// Draws the Keyclash app icon: one ⌘ key that two shortcuts want. The key fills the whole icon shape.
// It cracks down the middle, blue on one side and red on the other, with light in the crack.
// The ⌘ stays whole on top of the crack.
// Run: swift assets/make_icon.swift assets/icon-1024.png
// Other icon ideas: assets/icon-concepts/make_concepts.swift. Earlier icons are in Git history.
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

// The icon body: the macOS shape (824 of 1024, with room for the shadow). The key fills it.
let body = CGRect(x: 100, y: 100, width: 824, height: 824)
let bodyRadius: CGFloat = 186
let bodyPath = rounded(body, bodyRadius)
// The lit face of the key. The strip below it is the darker side of the key.
let depth: CGFloat = 58
let face = CGRect(x: body.minX, y: body.minY + depth, width: body.width, height: body.height - depth)
let facePath = rounded(face, bodyRadius)

ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: rgb(0x000000, 0.35))
ctx.addPath(bodyPath)
ctx.setFillColor(rgb(0x151821))
ctx.fillPath()
ctx.restoreGState()

// The crack runs from above the body to below it, so it splits the whole key.
let crackOffsets: [CGFloat] = [0, 40, -28, 46, -36, 24, -40, 10]
let crackTop = body.maxY + 4, crackBottom = body.minY - 4
let crack = crackOffsets.enumerated().map { i, x in
    CGPoint(x: 512 + x, y: crackTop + (crackBottom - crackTop) * CGFloat(i) / CGFloat(crackOffsets.count - 1))
}

ctx.saveGState()
ctx.addPath(bodyPath)
ctx.clip()

// The two halves: the side color under the whole body, then the face on top.
for (sign, faceColors, side) in [(CGFloat(-1), (UInt32(0x6DB4FF), UInt32(0x2B74E0)), UInt32(0x174A9C)),
                                 (CGFloat(1), (UInt32(0xFF8C7A), UInt32(0xE83F4D)), UInt32(0x9C1F2E))] {
    let half = CGMutablePath()
    half.move(to: CGPoint(x: 512 + sign * 2000, y: crackTop))
    // addLines(between:) would start a new subpath, so add each point to this one.
    for point in crack { half.addLine(to: point) }
    half.addLine(to: CGPoint(x: 512 + sign * 2000, y: crackBottom))
    half.closeSubpath()
    ctx.saveGState()
    ctx.addPath(half)
    ctx.clip()
    ctx.setFillColor(rgb(side))
    ctx.fill(body)
    ctx.addPath(facePath)
    ctx.clip()
    ctx.drawLinearGradient(grad([rgb(faceColors.0), rgb(faceColors.1)]), start: CGPoint(x: 0, y: face.maxY), end: CGPoint(x: 0, y: face.minY), options: [])
    ctx.restoreGState()
}

// A light edge around the face, and a thin shade where the face meets the side.
ctx.saveGState()
ctx.addPath(facePath)
ctx.clip()
ctx.addPath(rounded(face.insetBy(dx: 3, dy: 3), bodyRadius - 3))
ctx.setStrokeColor(rgb(0xFFFFFF, 0.30))
ctx.setLineWidth(6)
ctx.strokePath()
ctx.restoreGState()
ctx.saveGState()
// Only where the face meets the side: the rest of this line would sit on the body edge.
ctx.clip(to: CGRect(x: body.minX, y: body.minY, width: body.width, height: face.minY + bodyRadius - body.minY))
ctx.addPath(facePath)
ctx.setStrokeColor(rgb(0x000000, 0.22))
ctx.setLineWidth(3)
ctx.strokePath()
ctx.restoreGState()

// The light in the crack: an orange outer glow, a yellow inner glow, and a pale core.
let gap: CGFloat = 12
let wedge = CGMutablePath()
wedge.addLines(between: crack.map { CGPoint(x: $0.x - gap, y: $0.y) } + crack.reversed().map { CGPoint(x: $0.x + gap, y: $0.y) })
wedge.closeSubpath()
for (glowColor, blur) in [(UInt32(0xFF9A2E), CGFloat(70)), (UInt32(0xFFC94A), CGFloat(26))] {
    ctx.saveGState()
    ctx.setShadow(offset: .zero, blur: blur, color: rgb(glowColor, 1))
    ctx.addPath(wedge)
    ctx.setFillColor(rgb(0xFFC235))
    ctx.fillPath()
    ctx.restoreGState()
}
let core = CGMutablePath()
core.addLines(between: crack)
ctx.addPath(core)
ctx.setStrokeColor(rgb(0xFFF6D6))
ctx.setLineWidth(9)
ctx.setLineJoin(.round)
ctx.strokePath()

// A thin light edge around the whole body.
ctx.addPath(rounded(body.insetBy(dx: 2, dy: 2), bodyRadius - 2))
ctx.setStrokeColor(rgb(0xFFFFFF, 0.12))
ctx.setLineWidth(4)
ctx.strokePath()
ctx.restoreGState()

// The ⌘, whole, on top of the crack, centered on the face. A soft shadow lifts it off the light.
let font = NSFont.systemFont(ofSize: 410, weight: .semibold)
let text = NSAttributedString(string: "⌘", attributes: [.font: font, .foregroundColor: NSColor.white])
let line = CTLineCreateWithAttributedString(text)
let b = CTLineGetBoundsWithOptions(line, .useGlyphPathBounds)
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -9), blur: 22, color: rgb(0x000000, 0.40))
ctx.textPosition = CGPoint(x: face.midX - b.midX, y: face.midY - b.midY)
CTLineDraw(line, ctx)
ctx.restoreGState()

let image = ctx.makeImage()!
let rep = NSBitmapImageRep(cgImage: image)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out)")
