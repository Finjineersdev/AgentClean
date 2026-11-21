// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "CleanCore",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .library(name: "CleanCore", targets: ["CleanCore"]),
        .library(name: "AgentEngine", targets: ["AgentEngine"]),
    ],
    targets: [
        .target(
            name: "CleanCore",
            path: "Sources/CleanCore",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .target(
            name: "AgentEngine",
            path: "Sources/AgentEngine",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "AgentEngineTests",
            dependencies: ["AgentEngine", "CleanCore"],
            path: "Tests/AgentEngineTests",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
