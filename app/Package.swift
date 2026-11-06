// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Incant",
    platforms: [.macOS(.v15)],
    targets: [
        .executableTarget(
            name: "Incant",
            path: "Sources/Incant",
            linkerSettings: [.linkedFramework("Carbon")]
        )
    ]
)
