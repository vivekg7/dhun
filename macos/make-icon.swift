// Draws Dhun's mark as a macOS icon set: the play triangle cut into three
// slices (android/app/src/main/res/drawable/ic_glyph.xml) on the brand
// colour, in the rounded square every Mac icon shares.
//
//   swift macos/make-icon.swift OUT.iconset
import AppKit

let out = URL(fileURLWithPath: CommandLine.arguments[1])
try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)

func draw(_ px: Int) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8, samplesPerPixel: 4,
        hasAlpha: true,
        isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let ctx = NSGraphicsContext.current!.cgContext
    let s = CGFloat(px) / 1024
    // Apple's grid: an 824-unit square, inset 100, corners of 185.
    let tile = CGRect(x: 100 * s, y: 100 * s, width: 824 * s, height: 824 * s)
    ctx.addPath(CGPath(roundedRect: tile, cornerWidth: 185 * s, cornerHeight: 185 * s, transform: nil))
    ctx.setFillColor(CGColor(srgbRed: 0xC2 / 255, green: 0x7B / 255, blue: 0x45 / 255, alpha: 1))
    ctx.fillPath()
    // The glyph's 108-unit viewport, scaled onto the tile; y grows upward here.
    let k = tile.width / 108
    func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: tile.minX + x * k, y: tile.maxY - y * k) }
    ctx.saveGState()
    ctx.addRect(CGRect(origin: p(0, 45.05), size: CGSize(width: 108 * k, height: 45.05 * k)))
    ctx.addRect(CGRect(origin: p(0, 58.45), size: CGSize(width: 108 * k, height: 8.9 * k)))
    ctx.addRect(CGRect(origin: p(0, 108), size: CGSize(width: 108 * k, height: 45.05 * k)))
    ctx.clip()
    ctx.move(to: p(44, 34))
    ctx.addLine(to: p(76, 54))
    ctx.addLine(to: p(44, 74))
    ctx.closePath()
    ctx.setFillColor(.white)
    ctx.setStrokeColor(.white)
    ctx.setLineWidth(6 * k)
    ctx.setLineJoin(.round)
    ctx.drawPath(using: .fillStroke)
    ctx.restoreGState()
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

for size in [16, 32, 128, 256, 512] {
    try draw(size).write(to: out.appendingPathComponent("icon_\(size)x\(size).png"))
    try draw(size * 2).write(to: out.appendingPathComponent("icon_\(size)x\(size)@2x.png"))
}
