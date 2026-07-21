// swift-tools-version:5.9
// Minimal SwiftPM project fixture for milestone 122 US1 integration tests.
// `Package.swift` content is NEVER parsed by waybill in v0.1; this file
// exists only so the reader's manifest::detect() returns true and the
// sibling `Package.resolved` flows through the normal happy path.
import PackageDescription

let package = Package(
    name: "demo-swift",
    products: [
        .library(name: "demo-swift", targets: ["demo-swift"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-argument-parser.git", from: "1.3.0"),
        .package(url: "https://github.com/Alamofire/Alamofire.git", from: "5.9.0"),
    ],
    targets: [
        .target(name: "demo-swift", dependencies: ["swift-argument-parser", "Alamofire"]),
    ]
)
