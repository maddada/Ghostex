// swift-tools-version: 5.9

import PackageDescription

let package = Package(
  name: "GhostexEditor",
  platforms: [
    // `.v15` needs swift-tools-version 6.0; the string form works with 5.9.
    .macOS("15.0")
  ],
  products: [
    .executable(name: "GhostexEditor", targets: ["GhostexEditor"])
  ],
  targets: [
    .executableTarget(name: "GhostexEditor")
  ]
)
