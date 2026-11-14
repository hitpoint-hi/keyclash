// Draws the Keyclash app icon: two keycaps that want the same key, with a spark where they meet.
// Run: swift assets/make_icon.swift assets/icon-1024.png
import AppKit

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icon-1024.png"
let cs = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(data: nil, width: 1024, height: 1024, bitsPerComponent: 8, bytesPerRow: 0, space: cs,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!

func rgb(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: a)
}
func grad(_ colors: [CGColor], _ stops: [CGFloat]? = nil) -> CGGradient {
    CGGradient(colorsSpace: cs, colors: colors as CFArray, locations: stops)!
}
func rounded(_ r: CGRect, _ radius: CGFloat) -> CGPath {
    CGPath(roundedRect: r, cornerWidth: radius, cornerHeight: radius, transform: nil)
}

// The icon body: the macOS shape (824 of 1024, with room for the shadow).
let body = CGRect(x: 100, y: 100, width: 824, height: 824)
let bodyPath = rounded(body, 186)
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: rgb(0x000000, 0.35))
ctx.addPath(bodyPath)
ctx.setFillColor(rgb(0x151821))
ctx.fillPath()
ctx.restoreGState()

ctx.saveGState()
ctx.addPath(bodyPath)
ctx.clip()
ctx.drawLinearGradient(grad([rgb(0x2B3145), rgb(0x0F1219)]), start: CGPoint(x: 0, y: 924), end: CGPoint(x: 0, y: 100), options: [])
// Warm light from the point where the keys meet.
let hit = CGPoint(x: 512, y: 606)
ctx.drawRadialGradient(grad([rgb(0xFFC94A, 0.42), rgb(0xFF8A3D, 0.14), rgb(0xFF8A3D, 0)], [0, 0.45, 1]),
                       startCenter: hit, startRadius: 0, endCenter: hit, endRadius: 430, options: [])
ctx.restoreGState()
// A thin light edge at the top of the body.
ctx.saveGState()
ctx.addPath(bodyPath)
ctx.clip()
ctx.addPath(rounded(body.insetBy(dx: 2, dy: 2), 184))
ctx.setStrokeColor(rgb(0xFFFFFF, 0.10))
ctx.setLineWidth(4)
ctx.strokePath()
ctx.restoreGState()

/// One keycap: a darker side below, a lit face, and the key symbol.
func keycap(center: CGPoint, angle: CGFloat, face: (UInt32, UInt32), side: UInt32, glyph: String) {
    let w: CGFloat = 318, depth: CGFloat = 36, radius: CGFloat = 70
    ctx.saveGState()
    ctx.translateBy(x: center.x, y: center.y)
    ctx.rotate(by: angle)
    let faceRect = CGRect(x: -w / 2, y: -w / 2, width: w, height: w)
    // The side of the key, with the shadow of the whole key.
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -16), blur: 34, color: rgb(0x000000, 0.5))
    ctx.addPath(rounded(faceRect.offsetBy(dx: 0, dy: -depth), radius))
    ctx.setFillColor(rgb(side))
    ctx.fillPath()
    ctx.restoreGState()
    // The face.
    ctx.saveGState()
    ctx.addPath(rounded(faceRect, radius))
    ctx.clip()
    ctx.drawLinearGradient(grad([rgb(face.0), rgb(face.1)]), start: CGPoint(x: 0, y: w / 2), end: CGPoint(x: 0, y: -w / 2), options: [])
    ctx.addPath(rounded(faceRect.insetBy(dx: 3, dy: 3), radius - 3))
    ctx.setStrokeColor(rgb(0xFFFFFF, 0.28))
    ctx.setLineWidth(6)
    ctx.strokePath()
    ctx.restoreGState()
    // The symbol.
    let font = NSFont.systemFont(ofSize: 188, weight: .semibold)
    let text = NSAttributedString(string: glyph, attributes: [.font: font, .foregroundColor: NSColor.white])
    let line = CTLineCreateWithAttributedString(text)
    let b = CTLineGetBoundsWithOptions(line, .useGlyphPathBounds)
    ctx.setShadow(offset: CGSize(width: 0, height: -5), blur: 8, color: rgb(0x000000, 0.22))
    ctx.textPosition = CGPoint(x: -b.midX, y: -b.midY)
    CTLineDraw(line, ctx)
    ctx.restoreGState()
}

// The two keys lean in and meet at their top corners.
let lean: CGFloat = 0.21
keycap(center: CGPoint(x: 344, y: 432), angle: -lean, face: (0x6DB4FF, 0x2B74E0), side: 0x174A9C, glyph: "⌘")
keycap(center: CGPoint(x: 680, y: 432), angle: lean, face: (0xFF8C7A, 0xE83F4D), side: 0x9C1F2E, glyph: "⌘")

/// A jagged burst, as in a comic: longer points up, shorter points down where the keys are.
func burst(_ c: CGPoint, size: CGFloat) -> CGPath {
    let wobble: [CGFloat] = [1.0, 0.84, 1.06, 0.88, 0.98, 0.82, 1.04, 0.9, 1.0, 0.86, 1.02, 0.8]
    let p = CGMutablePath()
    let n = wobble.count
    for i in 0..<(n * 2) {
        let a = CGFloat(i) * .pi / CGFloat(n) + .pi / 2
        // Points that go down are shorter.
        let up = 0.72 + 0.28 * (sin(a) + 1) / 2
        let r = i % 2 == 0 ? size * wobble[i / 2] * up : size * 0.5 * up
        let pt = CGPoint(x: c.x + cos(a) * r, y: c.y + sin(a) * r)
        if i == 0 { p.move(to: pt) } else { p.addLine(to: pt) }
    }
    p.closeSubpath()
    return p
}

// The clash.
let shape = burst(hit, size: 150)
ctx.saveGState()
ctx.setShadow(offset: .zero, blur: 44, color: rgb(0xFF9A2E, 0.95))
ctx.addPath(shape)
ctx.setFillColor(rgb(0xFFC235))
ctx.fillPath()
ctx.restoreGState()
ctx.saveGState()
ctx.addPath(shape)
ctx.clip()
ctx.drawRadialGradient(grad([rgb(0xFFFFFF), rgb(0xFFF3B8), rgb(0xFFCB3D), rgb(0xFF9A2E)], [0, 0.34, 0.74, 1]),
                       startCenter: hit, startRadius: 0, endCenter: hit, endRadius: 158, options: [.drawsAfterEndLocation])
ctx.restoreGState()
// Short lines that fly up from the clash.
ctx.setStrokeColor(rgb(0xFFE08A))
ctx.setLineCap(.round)
for (deg, from, to, width) in [(34.0, 186.0, 222.0, 13.0), (62.0, 190.0, 232.0, 15.0), (90.0, 192.0, 228.0, 15.0), (118.0, 190.0, 232.0, 15.0), (146.0, 186.0, 222.0, 13.0)] {
    let a = CGFloat(deg) * .pi / 180
    ctx.setLineWidth(CGFloat(width))
    ctx.move(to: CGPoint(x: hit.x + cos(a) * CGFloat(from), y: hit.y + sin(a) * CGFloat(from)))
    ctx.addLine(to: CGPoint(x: hit.x + cos(a) * CGFloat(to), y: hit.y + sin(a) * CGFloat(to)))
    ctx.strokePath()
}

let image = ctx.makeImage()!
let rep = NSBitmapImageRep(cgImage: image)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out)")
