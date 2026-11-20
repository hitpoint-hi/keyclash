// Draws the Keyclash app icon: one ⌘ key that two shortcuts want. The key cracks down the middle,
// blue on one side and red on the other, with light in the crack. The ⌘ stays whole on top of the crack.
// Run: swift assets/make_icon.swift assets/icon-1024.png
// Other icon ideas: assets/icon-concepts/make_concepts.swift. The earlier two-keycap icon is in Git history.
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

// The key, in its own coordinates: origin at the center of its face.
let center = CGPoint(x: 512, y: 536)
let w: CGFloat = 480, depth: CGFloat = 50, radius: CGFloat = 108, gap: CGFloat = 9
let top = w / 2, bottom = -w / 2 - depth

ctx.saveGState()
ctx.addPath(bodyPath)
ctx.clip()
ctx.drawLinearGradient(grad([rgb(0x2B3145), rgb(0x0F1219)]), start: CGPoint(x: 0, y: 924), end: CGPoint(x: 0, y: 100), options: [])
// Warm light from the crack, in the spark yellow that src/theme.rs and the site also use.
let lightCenter = CGPoint(x: 512, y: center.y + 30)
ctx.drawRadialGradient(grad([rgb(0xFFC94A, 0.32), rgb(0xFF8A3D, 0.11), rgb(0xFF8A3D, 0)], [0, 0.45, 1]),
                       startCenter: lightCenter, startRadius: 0, endCenter: lightCenter, endRadius: 440, options: [])
// A thin light edge at the top of the body.
ctx.addPath(rounded(body.insetBy(dx: 2, dy: 2), 184))
ctx.setStrokeColor(rgb(0xFFFFFF, 0.10))
ctx.setLineWidth(4)
ctx.strokePath()
ctx.restoreGState()

ctx.saveGState()
ctx.translateBy(x: center.x, y: center.y)

// The crack runs from the top of the face to the bottom of the side.
let offsets: [CGFloat] = [0, 24, -16, 28, -22, 14, -24, 6]
let crack = offsets.enumerated().map { i, x in
    CGPoint(x: x, y: top + (bottom - top) * CGFloat(i) / CGFloat(offsets.count - 1))
}

// One shadow for the whole key, painted outside the key shape only.
// The halves cover the rest, and the gap between them stays clear for the light.
let keyShape = rounded(CGRect(x: -w / 2 - gap, y: bottom, width: w + 2 * gap, height: w), radius)
ctx.saveGState()
ctx.addRect(CGRect(x: -2000, y: -2000, width: 4000, height: 4000))
ctx.addPath(keyShape)
ctx.clip(using: .evenOdd)
ctx.setShadow(offset: CGSize(width: 0, height: -depth * 0.45), blur: depth, color: rgb(0x000000, 0.55))
ctx.addPath(keyShape)
ctx.setFillColor(rgb(0x000000))
ctx.fillPath()
ctx.restoreGState()

// The light in the crack: the space between the two halves.
let leftEdge = crack.map { CGPoint(x: $0.x - gap, y: $0.y) }
let rightEdge = crack.map { CGPoint(x: $0.x + gap, y: $0.y) }
let wedge = CGMutablePath()
wedge.addLines(between: leftEdge + rightEdge.reversed())
wedge.closeSubpath()
ctx.saveGState()
ctx.setShadow(offset: .zero, blur: 34, color: rgb(0xFF9A2E, 1))
ctx.addPath(wedge)
ctx.setFillColor(rgb(0xFFC235))
ctx.fillPath()
ctx.restoreGState()
ctx.saveGState()
let core = CGMutablePath()
core.addLines(between: crack)
ctx.addPath(core)
ctx.setStrokeColor(rgb(0xFFF6D6))
ctx.setLineWidth(7)
ctx.setLineJoin(.round)
ctx.strokePath()
ctx.restoreGState()

// The two halves: a darker side below and a lit face, each moved out by `gap`.
let faceRect = CGRect(x: -w / 2, y: -w / 2, width: w, height: w)
for (sign, face, side) in [(CGFloat(-1), (UInt32(0x6DB4FF), UInt32(0x2B74E0)), UInt32(0x174A9C)),
                           (CGFloat(1), (UInt32(0xFF8C7A), UInt32(0xE83F4D)), UInt32(0x9C1F2E))] {
    let half = CGMutablePath()
    half.move(to: CGPoint(x: sign * 2000, y: top + 2000))
    half.addLine(to: CGPoint(x: crack[0].x, y: top + 2000))
    // addLines(between:) would start a new subpath, so add each point to this one.
    for point in crack { half.addLine(to: point) }
    half.addLine(to: CGPoint(x: crack[crack.count - 1].x, y: bottom - 2000))
    half.addLine(to: CGPoint(x: sign * 2000, y: bottom - 2000))
    half.closeSubpath()
    ctx.saveGState()
    ctx.translateBy(x: sign * gap, y: 0)
    ctx.addPath(half)
    ctx.clip()
    ctx.addPath(rounded(faceRect.offsetBy(dx: 0, dy: -depth), radius))
    ctx.setFillColor(rgb(side))
    ctx.fillPath()
    ctx.addPath(rounded(faceRect, radius))
    ctx.clip()
    ctx.drawLinearGradient(grad([rgb(face.0), rgb(face.1)]), start: CGPoint(x: 0, y: w / 2), end: CGPoint(x: 0, y: -w / 2), options: [])
    ctx.addPath(rounded(faceRect.insetBy(dx: 3, dy: 3), radius - 3))
    ctx.setStrokeColor(rgb(0xFFFFFF, 0.28))
    ctx.setLineWidth(6)
    ctx.strokePath()
    ctx.restoreGState()
}

// The ⌘, whole, on top of the crack. A soft shadow lifts it off the light.
let font = NSFont.systemFont(ofSize: 250, weight: .semibold)
let text = NSAttributedString(string: "⌘", attributes: [.font: font, .foregroundColor: NSColor.white])
let line = CTLineCreateWithAttributedString(text)
let b = CTLineGetBoundsWithOptions(line, .useGlyphPathBounds)
ctx.setShadow(offset: CGSize(width: 0, height: -6), blur: 14, color: rgb(0x000000, 0.40))
ctx.textPosition = CGPoint(x: -b.midX, y: -b.midY)
CTLineDraw(line, ctx)
ctx.restoreGState()

let image = ctx.makeImage()!
let rep = NSBitmapImageRep(cgImage: image)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out)")
