// swift-tools-version: 5.7
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "RealtimeNoiseBridge",
    platforms: [
        .macOS(.v12)
    ],
    products: [
        .library(
            name: "RealtimeNoiseBridge",
            targets: ["RealtimeNoiseBridge"]
        ),
    ],
    dependencies: [],
    targets: [
        .target(
            name: "RealtimeNoiseBridge",
            dependencies: []
        ),
        .testTarget(
            name: "RealtimeNoiseBridgeTests",
            dependencies: ["RealtimeNoiseBridge"]
        ),
    ]
)
