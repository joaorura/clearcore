//===----------------------------------------------------------------------===//
//
// RingBuffer.swift
// RealtimeNoiseHAL
//
// Lock-free thread-safe atomic circular buffer for 48kHz Float32 mono samples.
// Strictly adheres to real-time audio constraints:
// - Zero memory allocations in audio callback path.
// - Zero blocking locks or mutexes.
// - Zero synchronous RPCs or I/O.
// - Fail-closed digital silence (all zeros) on underrun, absence, or generation mismatch.
// - Atomic generation invalidation on mode switch or engine restart.
// - Absolute zero raw audio leakage from hardware microphone.
//
//===----------------------------------------------------------------------===//

import Foundation

#if canImport(Darwin)
import Darwin
#endif

/// Lock-free, single-producer single-consumer circular buffer for 48kHz Float32 mono audio.
public final class RingBuffer {
    /// Buffer capacity in samples (must be a power of 2).
    public let capacity: Int
    private let mask: Int

    /// Contiguous pre-allocated sample memory.
    private let storage: UnsafeMutablePointer<Float>

    /// Monotonically increasing write position (in samples).
    private let writeIndex: UnsafeMutablePointer<UInt64>

    /// Monotonically increasing read position (in samples).
    private let readIndex: UnsafeMutablePointer<UInt64>

    /// Current active engine generation counter.
    private let generation: UnsafeMutablePointer<UInt64>

    /// Last sequence number written by the noise suppression engine.
    private let sequence: UnsafeMutablePointer<UInt64>

    /// Telemetry counters for underrun and overflow events.
    private let underrunCount: UnsafeMutablePointer<UInt64>
    private let overflowCount: UnsafeMutablePointer<UInt64>

    /// Initializes a lock-free circular buffer with pre-allocated memory.
    ///
    /// - Parameter capacity: Buffer size in samples. Default is 16,384 samples (~341ms at 48kHz).
    ///   Must be a power of 2.
    public init(capacity: Int = 16384) {
        precondition(capacity > 0 && (capacity & (capacity - 1)) == 0, "Capacity must be a power of two")
        self.capacity = capacity
        self.mask = capacity - 1

        self.storage = UnsafeMutablePointer<Float>.allocate(capacity: capacity)
        self.storage.initialize(repeating: 0.0, count: capacity)

        self.writeIndex = UnsafeMutablePointer<UInt64>.allocate(capacity: 1)
        self.writeIndex.initialize(to: 0)

        self.readIndex = UnsafeMutablePointer<UInt64>.allocate(capacity: 1)
        self.readIndex.initialize(to: 0)

        self.generation = UnsafeMutablePointer<UInt64>.allocate(capacity: 1)
        self.generation.initialize(to: 0)

        self.sequence = UnsafeMutablePointer<UInt64>.allocate(capacity: 1)
        self.sequence.initialize(to: 0)

        self.underrunCount = UnsafeMutablePointer<UInt64>.allocate(capacity: 1)
        self.underrunCount.initialize(to: 0)

        self.overflowCount = UnsafeMutablePointer<UInt64>.allocate(capacity: 1)
        self.overflowCount.initialize(to: 0)
    }

    deinit {
        storage.deallocate()
        writeIndex.deallocate()
        readIndex.deallocate()
        generation.deallocate()
        sequence.deallocate()
        underrunCount.deallocate()
        overflowCount.deallocate()
    }

    // MARK: - Memory Ordering Primitives

    @inline(__always)
    private func memoryBarrier() {
        #if canImport(Darwin)
        OSMemoryBarrier()
        #endif
    }

    // MARK: - Realtime Audio Operations (Zero Allocation, Lock-Free)

    /// Writes processed audio samples into the ring buffer.
    /// Called by the hidden output endpoint or engine writer.
    ///
    /// - Parameters:
    ///   - samples: Raw pointer to incoming 48kHz Float32 mono samples.
    ///   - count: Number of frames to write.
    ///   - sequence: Engine sequence number of the audio frame.
    ///   - incomingGeneration: Engine generation associated with the samples.
    /// - Returns: Number of samples successfully written.
    @discardableResult
    public func write(
        samples: UnsafePointer<Float>,
        count: Int,
        sequence: UInt64,
        generation incomingGeneration: UInt64
    ) -> Int {
        guard count > 0 else { return 0 }

        // Atomic check of generation
        let currentGen = self.generation.pointee
        if currentGen != incomingGeneration {
            // New generation detected from writer: atomically invalidate and restart sequence
            invalidate(newGeneration: incomingGeneration)
        }

        let w = self.writeIndex.pointee
        let r = self.readIndex.pointee
        let currentFill = Int(w - r)
        let availableSpace = capacity - currentFill

        if availableSpace < count {
            // Buffer full: advance read pointer to drop oldest stale frames (keep freshest denoised audio)
            let dropCount = count - availableSpace
            self.readIndex.pointee = r + UInt64(dropCount)
            self.overflowCount.pointee &+= 1
        }

        // Circular copy without heap allocation
        let start = Int(w & UInt64(mask))
        let firstChunk = min(count, capacity - start)
        let secondChunk = count - firstChunk

        storage.advanced(by: start).assign(from: samples, count: firstChunk)
        if secondChunk > 0 {
            storage.assign(from: samples.advanced(by: firstChunk), count: secondChunk)
        }

        self.sequence.pointee = sequence
        memoryBarrier()
        self.writeIndex.pointee = w + UInt64(count)

        return count
    }

    /// Reads processed audio samples from the ring buffer into the destination buffer.
    /// Called directly by the visible input endpoint CoreAudio HAL callback.
    ///
    /// CONTRACT: If there is an underrun, generation mismatch, or absence of engine data,
    /// this function outputs PURE DIGITAL SILENCE (zeros) and returns 0.
    /// Absolutely ZERO un-denoised audio from hardware is ever leaked.
    ///
    /// - Parameters:
    ///   - destination: Pre-allocated audio output buffer (48kHz Float32).
    ///   - count: Number of frames requested by CoreAudio HAL.
    ///   - expectedGeneration: The generation expected by the consumer.
    /// - Returns: Number of valid frames written. If 0, pure silence was filled into destination.
    @discardableResult
    public func read(
        into destination: UnsafeMutablePointer<Float>,
        count: Int,
        expectedGeneration: UInt64
    ) -> Int {
        guard count > 0 else { return 0 }

        let currentGen = self.generation.pointee
        memoryBarrier()

        // Fail-closed condition 1: Generation mismatch or un-warmed generation
        if currentGen == 0 || currentGen != expectedGeneration {
            destination.initialize(repeating: 0.0, count: count)
            self.underrunCount.pointee &+= 1
            return 0
        }

        let w = self.writeIndex.pointee
        memoryBarrier()
        let r = self.readIndex.pointee

        // Fail-closed condition 2: Inverted pointers or concurrently cleared
        guard w >= r else {
            destination.initialize(repeating: 0.0, count: count)
            self.underrunCount.pointee &+= 1
            return 0
        }

        let available = Int(w - r)

        // Fail-closed condition 3: Buffer underrun (engine lagging or halted)
        if available < count {
            destination.initialize(repeating: 0.0, count: count)
            self.underrunCount.pointee &+= 1
            return 0
        }

        // Read valid processed samples
        let start = Int(r & UInt64(mask))
        let firstChunk = min(count, capacity - start)
        let secondChunk = count - firstChunk

        destination.assign(from: storage.advanced(by: start), count: firstChunk)
        if secondChunk > 0 {
            destination.advanced(by: firstChunk).assign(from: storage, count: secondChunk)
        }

        memoryBarrier()
        self.readIndex.pointee = r + UInt64(count)

        return count
    }

    // MARK: - Control Plane Operations (Out-of-Callback)

    /// Atomically invalidates the buffer, setting a new generation and wiping all sample memory.
    /// Invoked upon engine restart, mode changes (Bypass/Active/Mute), or supervisor recovery.
    public func invalidate(newGeneration: UInt64) {
        clear(newGeneration: newGeneration)
    }

    /// Clears the ring buffer, atomically zeroing sample memory and resetting pointers so that
    /// any concurrent or subsequent read on visible input immediately outputs digital silence (all zeros).
    ///
    /// - Parameter newGeneration: Optional new generation counter to assign atomically.
    public func clear(newGeneration: UInt64? = nil) {
        memoryBarrier()
        self.readIndex.pointee = 0
        self.writeIndex.pointee = 0
        self.sequence.pointee = 0
        if let newGen = newGeneration {
            self.generation.pointee = newGen
        }
        memoryBarrier()
        self.storage.initialize(repeating: 0.0, count: capacity)
        memoryBarrier()
    }

    /// Returns telemetry diagnostics for monitoring without interfering with audio I/O.
    public func getDiagnostics() -> (
        available: Int,
        underruns: UInt64,
        overflows: UInt64,
        generation: UInt64,
        sequence: UInt64
    ) {
        memoryBarrier()
        let w = self.writeIndex.pointee
        let r = self.readIndex.pointee
        let gen = self.generation.pointee
        let seq = self.sequence.pointee
        let under = self.underrunCount.pointee
        let over = self.overflowCount.pointee
        let avail = max(0, Int(w - r))
        return (avail, under, over, gen, seq)
    }

    // MARK: - Invariant & Verification Tests

    /// Validates that when control generation changes or clear() is called, the ring buffer is wiped
    /// and any concurrent or subsequent read on visible input immediately outputs digital silence (all zeros).
    @discardableResult
    public static func testControlGenerationChangeClearsRingAndVisibleInputOutputsSilence() -> Bool {
        let ring = RingBuffer(capacity: 1024)
        let testCount = 480
        let gen1: UInt64 = 1

        let writeBuffer = UnsafeMutablePointer<Float>.allocate(capacity: testCount)
        defer { writeBuffer.deallocate() }
        writeBuffer.initialize(repeating: 0.75, count: testCount)

        ring.write(samples: writeBuffer, count: testCount, sequence: 1, generation: gen1)

        let readBuffer = UnsafeMutablePointer<Float>.allocate(capacity: testCount)
        defer { readBuffer.deallocate() }
        readBuffer.initialize(repeating: -1.0, count: testCount)

        let readFrames1 = ring.read(into: readBuffer, count: testCount, expectedGeneration: gen1)
        guard readFrames1 == testCount && readBuffer.pointee == 0.75 else {
            return false
        }

        // Fill with fresh audio
        ring.write(samples: writeBuffer, count: testCount, sequence: 2, generation: gen1)

        // Invalidate generation / clear ring buffer
        let gen2: UInt64 = 2
        ring.clear(newGeneration: gen2)

        // Read with old generation: must output pure digital silence (zeros)
        readBuffer.initialize(repeating: 99.0, count: testCount)
        let readFramesOld = ring.read(into: readBuffer, count: testCount, expectedGeneration: gen1)
        guard readFramesOld == 0 else { return false }
        for i in 0..<testCount {
            guard readBuffer[i] == 0.0 else { return false }
        }

        // Read with new generation before writer writes: must output pure digital silence (zeros)
        readBuffer.initialize(repeating: 99.0, count: testCount)
        let readFramesNew = ring.read(into: readBuffer, count: testCount, expectedGeneration: gen2)
        guard readFramesNew == 0 else { return false }
        for i in 0..<testCount {
            guard readBuffer[i] == 0.0 else { return false }
        }

        return true
    }
}
