// swift-tools-version:5.9
// iOS-side SwiftPM project nested under the KMP root. The Gradle reader
// never sees Package.swift; the Swift reader never sees build.gradle.kts.
// Each reader contributes to the SAME emitted SBOM via the existing
// scan_fs::package_db::read_all dispatcher composition (FR-008).
import PackageDescription

let package = Package(
    name: "iosApp",
    dependencies: [
        .package(url: "https://github.com/Alamofire/Alamofire.git", from: "5.9.0"),
        .package(url: "https://github.com/apple/swift-log.git", from: "1.5.0"),
    ]
)
