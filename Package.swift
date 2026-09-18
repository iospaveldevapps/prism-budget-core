// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "BudgetCore",
    platforms: [
        // The consuming app targets a much newer iOS; the package itself only
        // needs a floor recent enough for the UniFFI-generated code. macOS is
        // included so `swift test` runs natively on development machines and CI.
        .iOS(.v16),
        .macOS(.v13)
    ],
    products: [
        .library(
            name: "BudgetCore",
            targets: ["BudgetCore"]
        )
    ],
    targets: [
        // Pre-built Rust static library (device + simulator + macOS slices),
        // produced by scripts/build-xcframework.sh and committed so consumers
        // never need a Rust toolchain.
        .binaryTarget(
            name: "budget_coreFFI",
            path: "budget_coreFFI.xcframework"
        ),
        // UniFFI-generated Swift bindings over the C FFI.
        .target(
            name: "BudgetCore",
            dependencies: [
                .target(name: "budget_coreFFI")
            ]
        ),
        .testTarget(
            name: "BudgetCoreTests",
            dependencies: ["BudgetCore"]
        )
    ]
)
