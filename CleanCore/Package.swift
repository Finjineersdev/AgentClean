// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "CleanCore",
    platforms: [
        .macOS("26.0")
    ],
    products: [
        .library(name: "CleanCore", targets: ["CleanCore"]),
        .library(name: "AgentEngine", targets: ["AgentEngine"]),
    ],
    targets: [
        .target(
            name: "CHIDSensors",
            path: "Sources/CHIDSensors",
            linkerSettings: [.linkedFramework("IOKit"), .linkedFramework("CoreFoundation")]
        ),
        .target(
            name: "CleanCore",
            dependencies: ["CHIDSensors"],
            path: "Sources/CleanCore",
            swiftSettings: [.swiftLanguageMode(.v6)],
            linkerSettings: [.linkedFramework("IOKit")]
        ),
        .target(
            name: "AgentEngine",
            path: "Sources/AgentEngine",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "AgentEngineTests",
            dependencies: ["AgentEngine"],
            path: "Tests/AgentEngineTests",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "CleanCoreTests",
            dependencies: ["CleanCore"],
            path: "Tests/CleanCoreTests",
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
