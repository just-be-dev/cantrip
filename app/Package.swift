// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Cantrip",
    platforms: [.macOS(.v15)],
    targets: [
        .executableTarget(
            name: "Cantrip",
            path: "Sources/Cantrip",
            linkerSettings: [.linkedFramework("Carbon")]
        )
    ]
)
