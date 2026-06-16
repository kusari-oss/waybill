// swift-tools-version:5.9
// Commit-pinned SwiftPM fixture for milestone 122 US1 AS3.
// `Package.swift` content is detected but never parsed.
import PackageDescription

let package = Package(
    name: "commit-pinned-demo",
    dependencies: [
        .package(url: "https://github.com/apple/swift-log.git", branch: "main"),
    ]
)
