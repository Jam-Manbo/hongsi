// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "tauri-plugin-hongsi-ios",
    platforms: [.iOS(.v17)],
    products: [.library(name: "tauri-plugin-hongsi-ios", type: .static, targets: ["HongsiIOS"])],
    dependencies: [.package(name: "Tauri", path: "../.tauri/tauri-api")],
    targets: [.target(name: "HongsiIOS", dependencies: [.byName(name: "Tauri")], path: "Sources")]
)
