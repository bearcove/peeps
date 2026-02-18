// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "rust-swift-peer",
    platforms: [
        .macOS(.v14)
    ],
    dependencies: [
        .package(path: "../../../../roam/swift/roam-runtime")
    ],
    targets: [
        .executableTarget(
            name: "rust_swift_peer",
            dependencies: [
                .product(name: "RoamRuntime", package: "roam-runtime")
            ]
        )
    ]
)
