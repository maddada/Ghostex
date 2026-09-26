// Renders the poster image of every live glass style from the same Metal source the app draws
// (.dependencies/zed/crates/gpui_macos/src/live_backdrop.metal), so the Theme page's style cards
// show exactly what the window will draw. Run from the repo root:
//
//   swift tooling/glass-live/render-posters.swift            (writes the posters)
//   swift tooling/glass-live/render-posters.swift --measure  (checks the styles, writes nothing)
//
// It writes packages/core-ui/assets/glass-live/<style>-<appearance>.jpg. Re-run it whenever a
// style changes. The poster colours are a sample (the Blue theme); the window paints each style in
// the current theme's colours.
//
// --measure prints, for every style, how bright it draws above its base colour (averaged over the
// loop, to set the LIVE_GAIN_* constants so every style sits at the same brightness) and how far
// the frame at the very end of the loop is from the frame at its start, next to one ordinary
// 1/24 s step: the end of the loop must be no further than an ordinary step, or the loop jumps.

import AppKit
import Metal

let styles = ["aurora", "ink", "drift", "nebula", "silk", "bokeh", "waves", "mesh"]
let width = 480
let height = 300
// A moment of the loop where every style shows its character.
let posterPhase: Float = 19.4
// LIVE_PERIOD in window_live.rs.
let period: Float = 120.0
let measure = CommandLine.arguments.contains("--measure")
// Settings' default Brightness (`windowGlassLiveBrightness`, 45%) for --measure; posters use 100%.
let drawBrightness: Float = measure ? 0.45 : 1.0

struct Uniforms {
    var resolution: SIMD2<Float>
    var viewSize: SIMD2<Float>
    var coverOrigin: SIMD2<Float>
    var coverSize: SIMD2<Float>
    var phase: Float
    var period: Float
    var brightness: Float = 1
    var pad: Float = 0
    var c0: SIMD4<Float>
    var c1: SIMD4<Float>
    var c2: SIMD4<Float>
}

func rgb(_ hex: UInt32) -> SIMD4<Float> {
    SIMD4(Float((hex >> 16) & 0xff) / 255, Float((hex >> 8) & 0xff) / 255, Float(hex & 0xff) / 255, 1)
}

// Dark and light samples in the shape `live_background_colors` (window_glass_live.rs) produces.
let palettes: [(String, [SIMD4<Float>])] = [
    ("dark", [rgb(0x0e1522), rgb(0x2f5d8f), rgb(0x5fb2e6)]),
    ("light", [rgb(0xe9eef5), rgb(0xa9c3e3), rgb(0x79b9e0)]),
]

let root = FileManager.default.currentDirectoryPath
let sourcePath = root + "/.dependencies/zed/crates/gpui_macos/src/live_backdrop.metal"
let outputFolder = root + "/packages/core-ui/assets/glass-live"
guard let source = try? String(contentsOfFile: sourcePath, encoding: .utf8) else {
    fatalError("cannot read \(sourcePath); run from the repo root")
}
guard let device = MTLCreateSystemDefaultDevice(), let queue = device.makeCommandQueue() else {
    fatalError("no Metal device")
}
let library: MTLLibrary
do {
    library = try device.makeLibrary(source: source, options: nil)
} catch {
    fatalError("live_backdrop.metal does not compile: \(error)")
}
try FileManager.default.createDirectory(atPath: outputFolder, withIntermediateDirectories: true)

func render(_ pipeline: MTLRenderPipelineState, _ colors: [SIMD4<Float>], phase: Float) -> [UInt8] {
    let textureDescriptor = MTLTextureDescriptor.texture2DDescriptor(
        pixelFormat: .bgra8Unorm, width: width, height: height, mipmapped: false)
    textureDescriptor.usage = [.renderTarget, .shaderRead]
    textureDescriptor.storageMode = .shared
    let texture = device.makeTexture(descriptor: textureDescriptor)!
    let pass = MTLRenderPassDescriptor()
    pass.colorAttachments[0].texture = texture
    pass.colorAttachments[0].loadAction = .clear
    pass.colorAttachments[0].storeAction = .store
    var uniforms = Uniforms(
        resolution: SIMD2(Float(width), Float(height)),
        viewSize: SIMD2(Float(width), Float(height)),
        coverOrigin: SIMD2(0, 0),
        coverSize: SIMD2(Float(width), Float(height)),
        phase: phase, period: period, brightness: drawBrightness,
        c0: colors[0], c1: colors[1], c2: colors[2])
    let commands = queue.makeCommandBuffer()!
    let encoder = commands.makeRenderCommandEncoder(descriptor: pass)!
    encoder.setRenderPipelineState(pipeline)
    encoder.setFragmentBytes(&uniforms, length: MemoryLayout<Uniforms>.stride, index: 0)
    encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
    encoder.endEncoding()
    commands.commit()
    commands.waitUntilCompleted()
    var pixels = [UInt8](repeating: 0, count: width * height * 4)
    texture.getBytes(
        &pixels, bytesPerRow: width * 4, from: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0)
    return pixels
}

func luminance(_ pixels: [UInt8]) -> Double {
    var total = 0.0
    for index in stride(from: 0, to: pixels.count, by: 4) {
        // BGRA.
        total += 0.0722 * Double(pixels[index]) + 0.7152 * Double(pixels[index + 1]) + 0.2126 * Double(pixels[index + 2])
    }
    return total / Double(pixels.count / 4) / 255.0
}

func meanDifference(_ a: [UInt8], _ b: [UInt8]) -> Double {
    var total = 0.0
    for index in 0..<a.count where index % 4 != 3 {
        total += abs(Double(a[index]) - Double(b[index]))
    }
    return total / Double(a.count / 4 * 3)
}

for style in styles {
    let descriptor = MTLRenderPipelineDescriptor()
    descriptor.vertexFunction = library.makeFunction(name: "live_vertex")
    descriptor.fragmentFunction = library.makeFunction(name: "live_\(style)")
    descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm
    let pipeline = try device.makeRenderPipelineState(descriptor: descriptor)
    if measure {
        let colors = palettes[0].1
        let base = luminance(render(pipeline, [colors[0], colors[0], colors[0]], phase: 0))
        let samples = (0..<12).map { luminance(render(pipeline, colors, phase: Float($0) * period / 12)) }
        let above = samples.reduce(0, +) / Double(samples.count) - base
        let loopEnd = render(pipeline, colors, phase: period - 0.0005)
        let loopStart = render(pipeline, colors, phase: 0)
        let oneStep = render(pipeline, colors, phase: 1.0 / 24.0)
        print(String(
            format: "%@ brightness_above_base=%.4f loop_seam_diff=%.4f one_frame_step_diff=%.4f",
            style, above, meanDifference(loopEnd, loopStart), meanDifference(loopStart, oneStep)))
        continue
    }
    for (appearance, colors) in palettes {
        var pixels = render(pipeline, colors, phase: posterPhase)
        // BGRA to RGBA.
        for index in stride(from: 0, to: pixels.count, by: 4) {
            pixels.swapAt(index, index + 2)
        }
        let provider = CGDataProvider(data: Data(pixels) as CFData)!
        let image = CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)!
        let bitmap = NSBitmapImageRep(cgImage: image)
        let data = bitmap.representation(using: .jpeg, properties: [.compressionFactor: 0.82])!
        let path = "\(outputFolder)/\(style)-\(appearance).jpg"
        try data.write(to: URL(fileURLWithPath: path))
        print("wrote \(path) (\(data.count / 1024) KB)")
    }
}
