// swift-tools-version: 5.9

import PackageDescription

let package = Package(
  name: "GhostexEditor",
  platforms: [
    .macOS(.v13)
  ],
  products: [
    .executable(name: "GhostexEditor", targets: ["GhostexEditor"])
  ],
  targets: [
    .executableTarget(name: "GhostexEditor")
  ]
)
