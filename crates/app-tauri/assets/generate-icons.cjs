#!/usr/bin/env node
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');

function crc32(buf) {
  let crc = -1;
  for (let i = 0; i < buf.length; i++) {
    crc ^= buf[i];
    for (let j = 0; j < 8; j++) {
      crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
    }
  }
  return (crc ^ -1) >>> 0;
}

function makeChunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, 'ascii');
  const crcBuf = Buffer.alloc(4);
  const crcVal = crc32(Buffer.concat([typeBuf, data]));
  crcBuf.writeUInt32BE(crcVal, 0);
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}

function createPng(width, height, drawFn) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  ihdr[10] = 0; // compression
  ihdr[11] = 0; // filter
  ihdr[12] = 0; // interlace

  const rowSize = 1 + width * 4;
  const rawData = Buffer.alloc(rowSize * height);

  for (let y = 0; y < height; y++) {
    const rowOffset = y * rowSize;
    rawData[rowOffset] = 0; // filter None
    for (let x = 0; x < width; x++) {
      const pxOffset = rowOffset + 1 + x * 4;
      const [r, g, b, a] = drawFn(x, y, width, height);
      rawData[pxOffset] = r;
      rawData[pxOffset + 1] = g;
      rawData[pxOffset + 2] = b;
      rawData[pxOffset + 3] = a;
    }
  }

  const idatData = zlib.deflateSync(rawData);
  const idatChunk = makeChunk('IDAT', idatData);
  const ihdrChunk = makeChunk('IHDR', ihdr);
  const iendChunk = makeChunk('IEND', Buffer.alloc(0));

  return Buffer.concat([signature, ihdrChunk, idatChunk, iendChunk]);
}

// Function to draw a clean microphone with a colored status badge
function drawMicIcon(statusColor) {
  return (x, y, w, h) => {
    // Normalize coordinates to 0..32
    const nx = (x / w) * 32;
    const ny = (y / h) * 32;

    // Mic capsule: rounded rectangle at center x=16, y=6..18, radius=4
    const inCapsule = (nx >= 12 && nx <= 20 && ny >= 6 && ny <= 18);
    const inCapsuleTop = (Math.hypot(nx - 16, ny - 6) <= 4);
    const inCapsuleBottom = (Math.hypot(nx - 16, ny - 18) <= 4);
    const isMicBody = inCapsule || inCapsuleTop || inCapsuleBottom;

    // Mic cradle: arc from y=12 to y=20, x from 8 to 24
    const cradleDist = Math.hypot(nx - 16, ny - 16);
    const isCradle = (cradleDist >= 6.5 && cradleDist <= 8.5 && ny >= 14 && ny <= 23);

    // Mic stand: stem at x=16, y=23..27; base at x=11..21, y=27..29
    const isStem = (nx >= 15 && nx <= 17 && ny >= 23 && ny <= 27);
    const isBase = (nx >= 10 && nx <= 22 && ny >= 27 && ny <= 29);

    // Status dot badge at bottom-right (x=24, y=24, radius=5)
    const badgeDist = Math.hypot(nx - 24, ny - 24);
    const isBadgeBorder = (badgeDist <= 5.5 && badgeDist > 4.0);
    const isBadge = (badgeDist <= 4.0);

    if (isBadge) {
      return statusColor; // Status color (Green, Yellow, Red)
    }
    if (isBadgeBorder) {
      return [20, 20, 24, 255]; // Dark outline for badge
    }

    if (isMicBody) {
      return [235, 238, 245, 255]; // Bright silver/white
    }
    if (isCradle || isStem || isBase) {
      return [160, 170, 185, 255]; // Metallic accent
    }

    return [0, 0, 0, 0]; // Transparent background
  };
}

const assetsDir = path.join(__dirname);
fs.mkdirSync(assetsDir, { recursive: true });

// 1. Main Clearcore App Icon (Cyan / Indigo badge)
const appIcon = createPng(64, 64, drawMicIcon([14, 165, 233, 255])); // Cyan
fs.writeFileSync(path.join(assetsDir, 'icon.png'), appIcon);

// 2. Active Mode Tray Icon (Green badge: suppression active)
const activeTray = createPng(32, 32, drawMicIcon([34, 197, 94, 255])); // Green #22c55e
fs.writeFileSync(path.join(assetsDir, 'tray-active.png'), activeTray);

// 3. Bypass Mode Tray Icon (Amber / Yellow badge: passthrough)
const bypassTray = createPng(32, 32, drawMicIcon([234, 179, 8, 255])); // Yellow #eab308
fs.writeFileSync(path.join(assetsDir, 'tray-bypass.png'), bypassTray);

// 4. Mute Mode Tray Icon (Red badge: digital silence)
const muteTray = createPng(32, 32, drawMicIcon([239, 68, 68, 255])); // Red #ef4444
fs.writeFileSync(path.join(assetsDir, 'tray-mute.png'), muteTray);

console.log('Successfully generated tray and app icons in:', assetsDir);
