// Draws alternate Keyclash app icon concepts, one PNG (1024 x 1024) for each.
// The current icon stays in assets/make_icon.swift. These are options to compare.
// Run: swift assets/icon-concepts/make_concepts.swift assets/icon-concepts
import AppKit

let outDir = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
let cs = CGColorSpace(name: CGColorSpace.sRGB)!

func rgb(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: a)
}
func grad(_ colors: [CGColor], _ stops: [CGFloat]? = nil) -> CGGradient {
    CGGradient(colorsSpace: cs, colors: colors as CFArray, locations: stops)!
}
func rounded(_ r: CGRect, _ radius: CGFloat) -> CGPath {
    CGPath(roundedRect: r, cornerWidth: radius, cornerHeight: radius, transform: nil)
}
func newContext() -> CGContext {
    CGContext(data: nil, width: 1024, height: 1024, bitsPerComponent: 8, bytesPerRow: 0, space: cs,
              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
}
func save(_ ctx: CGContext, _ path: String) {
    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    guard let png = rep.representation(using: .png, properties: [:]) else {
        FileHandle.standardError.write("cannot encode \(path) as PNG\n".data(using: .utf8)!)
        exit(1)
    }
    do {
        try png.write(to: URL(fileURLWithPath: path))
    } catch {
        FileHandle.standardError.write("cannot write \(path): \(error.localizedDescription)\n".data(using: .utf8)!)
        exit(1)
    }
    print("wrote \(path)")
}

// The icon body: the macOS shape (824 of 1024, with room for the shadow). Same as the current icon.
let body = CGRect(x: 100, y: 100, width: 824, height: 824)
let bodyRadius: CGFloat = 186

/// Fills the body with a vertical gradient, runs `inside` clipped to the body, and adds a thin light edge.
func drawBody(_ ctx: CGContext, top: UInt32, bottom: UInt32, edge: CGFloat = 0.10, inside: (CGContext) -> Void = { _ in }) {
    let path = rounded(body, bodyRadius)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: rgb(0x000000, 0.35))
    ctx.addPath(path)
    ctx.setFillColor(rgb(bottom))
    ctx.fillPath()
    ctx.restoreGState()

    ctx.saveGState()
    ctx.addPath(path)
    ctx.clip()
    ctx.drawLinearGradient(grad([rgb(top), rgb(bottom)]), start: CGPoint(x: 0, y: 924), end: CGPoint(x: 0, y: 100), options: [])
    inside(ctx)
    ctx.addPath(rounded(body.insetBy(dx: 2, dy: 2), bodyRadius - 2))
    ctx.setStrokeColor(rgb(0xFFFFFF, edge))
    ctx.setLineWidth(4)
    ctx.strokePath()
    ctx.restoreGState()
}

func glow(_ ctx: CGContext, at p: CGPoint, radius: CGFloat, color: UInt32, alpha: CGFloat) {
    ctx.drawRadialGradient(grad([rgb(color, alpha), rgb(color, alpha * 0.35), rgb(color, 0)], [0, 0.45, 1]),
                           startCenter: p, startRadius: 0, endCenter: p, endRadius: radius, options: [])
}

/// Draws a symbol centered on `p`, using the bounds of its glyph paths.
func glyph(_ ctx: CGContext, _ s: String, at p: CGPoint, size: CGFloat, weight: NSFont.Weight = .semibold,
           color: CGColor, shadow: CGColor? = nil, shadowBlur: CGFloat = 8, shadowY: CGFloat = -5) {
    let font = NSFont.systemFont(ofSize: size, weight: weight)
    let text = NSAttributedString(string: s, attributes: [.font: font, .foregroundColor: NSColor(cgColor: color)!])
    let line = CTLineCreateWithAttributedString(text)
    let b = CTLineGetBoundsWithOptions(line, .useGlyphPathBounds)
    ctx.saveGState()
    if let shadow { ctx.setShadow(offset: CGSize(width: 0, height: shadowY), blur: shadowBlur, color: shadow) }
    ctx.textPosition = CGPoint(x: p.x - b.midX, y: p.y - b.midY)
    CTLineDraw(line, ctx)
    ctx.restoreGState()
}

/// One keycap: a darker side below, a lit face, and a legend drawn in the key's own coordinates (origin at the face center).
func keycap(_ ctx: CGContext, center: CGPoint, angle: CGFloat = 0, w: CGFloat, depth: CGFloat, radius: CGFloat,
            face: (UInt32, UInt32), side: UInt32, rim: CGColor = rgb(0xFFFFFF, 0.28), shadowAlpha: CGFloat = 0.5,
            legend: (CGContext) -> Void = { _ in }) {
    ctx.saveGState()
    ctx.translateBy(x: center.x, y: center.y)
    ctx.rotate(by: angle)
    let faceRect = CGRect(x: -w / 2, y: -w / 2, width: w, height: w)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -depth * 0.45), blur: depth, color: rgb(0x000000, shadowAlpha))
    ctx.addPath(rounded(faceRect.offsetBy(dx: 0, dy: -depth), radius))
    ctx.setFillColor(rgb(side))
    ctx.fillPath()
    ctx.restoreGState()
    ctx.saveGState()
    ctx.addPath(rounded(faceRect, radius))
    ctx.clip()
    ctx.drawLinearGradient(grad([rgb(face.0), rgb(face.1)]), start: CGPoint(x: 0, y: w / 2), end: CGPoint(x: 0, y: -w / 2), options: [])
    ctx.addPath(rounded(faceRect.insetBy(dx: 3, dy: 3), radius - 3))
    ctx.setStrokeColor(rim)
    ctx.setLineWidth(6)
    ctx.strokePath()
    ctx.restoreGState()
    legend(ctx)
    ctx.restoreGState()
}

/// A jagged burst, as in a comic: longer points up, shorter points down. Same shape as the current icon.
func burst(_ c: CGPoint, size: CGFloat) -> CGPath {
    let wobble: [CGFloat] = [1.0, 0.84, 1.06, 0.88, 0.98, 0.82, 1.04, 0.9, 1.0, 0.86, 1.02, 0.8]
    let p = CGMutablePath()
    let n = wobble.count
    for i in 0..<(n * 2) {
        let a = CGFloat(i) * .pi / CGFloat(n) + .pi / 2
        let up = 0.72 + 0.28 * (sin(a) + 1) / 2
        let r = i % 2 == 0 ? size * wobble[i / 2] * up : size * 0.5 * up
        let pt = CGPoint(x: c.x + cos(a) * r, y: c.y + sin(a) * r)
        if i == 0 { p.move(to: pt) } else { p.addLine(to: pt) }
    }
    p.closeSubpath()
    return p
}

func drawBurst(_ ctx: CGContext, at c: CGPoint, size: CGFloat) {
    let shape = burst(c, size: size)
    ctx.saveGState()
    ctx.setShadow(offset: .zero, blur: size * 0.3, color: rgb(0xFF9A2E, 0.95))
    ctx.addPath(shape)
    ctx.setFillColor(rgb(0xFFC235))
    ctx.fillPath()
    ctx.restoreGState()
    ctx.saveGState()
    ctx.addPath(shape)
    ctx.clip()
    ctx.drawRadialGradient(grad([rgb(0xFFFFFF), rgb(0xFFF3B8), rgb(0xFFCB3D), rgb(0xFF9A2E)], [0, 0.34, 0.74, 1]),
                           startCenter: c, startRadius: 0, endCenter: c, endRadius: size * 1.05, options: [.drawsAfterEndLocation])
    ctx.restoreGState()
}

/// Short lines that fly out from a point. Each entry: angle in degrees, start radius, end radius, width.
func sparks(_ ctx: CGContext, from c: CGPoint, color: UInt32, _ lines: [(Double, Double, Double, Double)]) {
    ctx.saveGState()
    ctx.setStrokeColor(rgb(color))
    ctx.setLineCap(.round)
    for (deg, from, to, width) in lines {
        let a = CGFloat(deg) * .pi / 180
        ctx.setLineWidth(CGFloat(width))
        ctx.move(to: CGPoint(x: c.x + cos(a) * CGFloat(from), y: c.y + sin(a) * CGFloat(from)))
        ctx.addLine(to: CGPoint(x: c.x + cos(a) * CGFloat(to), y: c.y + sin(a) * CGFloat(to)))
        ctx.strokePath()
    }
    ctx.restoreGState()
}

/// A straight round-capped bar filled with a vertical gradient, with a drop shadow.
func bar(_ ctx: CGContext, from a: CGPoint, to b: CGPoint, width: CGFloat, colors: (UInt32, UInt32)) {
    let line = CGMutablePath()
    line.move(to: a)
    line.addLine(to: b)
    let shape = line.copy(strokingWithWidth: width, lineCap: .round, lineJoin: .round, miterLimit: 10)
    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 26, color: rgb(0x000000, 0.45))
    ctx.addPath(shape)
    ctx.setFillColor(rgb(colors.1))
    ctx.fillPath()
    ctx.restoreGState()
    ctx.saveGState()
    ctx.addPath(shape)
    ctx.clip()
    let top = max(a.y, b.y) + width / 2, bottom = min(a.y, b.y) - width / 2
    ctx.drawLinearGradient(grad([rgb(colors.0), rgb(colors.1)]), start: CGPoint(x: 0, y: top), end: CGPoint(x: 0, y: bottom), options: [])
    ctx.restoreGState()
}

let blueFace: (UInt32, UInt32) = (0x6DB4FF, 0x2B74E0), blueSide: UInt32 = 0x174A9C
let redFace: (UInt32, UInt32) = (0xFF8C7A, 0xE83F4D), redSide: UInt32 = 0x9C1F2E

// MARK: - Concepts

/// A. One key that two shortcuts own: it cracks down the middle, blue on one side and red on the other.
/// - light: a light body for a light Dock.
/// - tilt: how far each half leans out from the bottom, in radians. 0 keeps the halves upright.
/// - zigzag: false gives a straight cut.
/// - spark: short lines of light fly up out of the top of the crack.
func splitKey(_ ctx: CGContext, light: Bool = false, tilt: CGFloat = 0, zigzag: Bool = true, spark: Bool = false) {
    let center = CGPoint(x: 512, y: spark ? 514 : 536)
    if light {
        drawBody(ctx, top: 0xFFFFFF, bottom: 0xDFE4EB, edge: 0.9) { c in
            glow(c, at: CGPoint(x: 512, y: center.y + 30), radius: 440, color: 0xFFB13D, alpha: 0.26)
        }
    } else {
        drawBody(ctx, top: 0x2B3145, bottom: 0x0F1219) { c in
            glow(c, at: CGPoint(x: 512, y: center.y + 30), radius: 440, color: 0xFFB13D, alpha: 0.30)
        }
    }
    let w: CGFloat = 480, depth: CGFloat = 50, radius: CGFloat = 108, gap: CGFloat = 9
    ctx.saveGState()
    ctx.translateBy(x: center.x, y: center.y)
    let top = w / 2, bottom = -w / 2 - depth
    let offsets: [CGFloat] = zigzag ? [0, 24, -16, 28, -22, 14, -24, 6] : [0, 0]
    let crack = offsets.enumerated().map { i, x in
        CGPoint(x: x, y: top + (bottom - top) * CGFloat(i) / CGFloat(offsets.count - 1))
    }
    // Each half moves out by `gap`, then leans out about the bottom of the crack.
    func place(_ sign: CGFloat) -> CGAffineTransform {
        CGAffineTransform(translationX: 0, y: bottom).rotated(by: -sign * tilt)
            .translatedBy(x: 0, y: -bottom).translatedBy(x: sign * gap, y: 0)
    }
    // The light in the crack: the space between the two placed halves.
    let leftEdge = crack.map { $0.applying(place(-1)) }
    let rightEdge = crack.map { $0.applying(place(1)) }
    let wedge = CGMutablePath()
    wedge.addLines(between: leftEdge + rightEdge.reversed())
    wedge.closeSubpath()
    // One shadow for the whole key, so no light strip shows under the crack.
    // Paint it outside the key shape only: the halves cover the rest, and the gap stays clear for the light.
    let shadowAlpha: CGFloat = light ? 0.32 : 0.55
    let keyShape = rounded(CGRect(x: -w / 2 - gap, y: bottom, width: w + 2 * gap, height: w), radius)
    ctx.saveGState()
    ctx.addRect(CGRect(x: -2000, y: -2000, width: 4000, height: 4000))
    ctx.addPath(keyShape)
    ctx.clip(using: .evenOdd)
    ctx.setShadow(offset: CGSize(width: 0, height: -depth * 0.45), blur: depth, color: rgb(0x000000, shadowAlpha))
    ctx.addPath(keyShape)
    ctx.setFillColor(rgb(0x000000))
    ctx.fillPath()
    ctx.restoreGState()
    ctx.saveGState()
    ctx.setShadow(offset: .zero, blur: 34, color: rgb(0xFF9A2E, 1))
    ctx.addPath(wedge)
    ctx.setFillColor(rgb(0xFFC235))
    ctx.fillPath()
    ctx.restoreGState()
    let core = CGMutablePath()
    core.addLines(between: zip(leftEdge, rightEdge).map { CGPoint(x: ($0.x + $1.x) / 2, y: ($0.y + $1.y) / 2) })
    ctx.saveGState()
    ctx.addPath(core)
    ctx.setStrokeColor(rgb(0xFFF6D6))
    ctx.setLineWidth(7)
    ctx.setLineJoin(.round)
    ctx.setLineCap(.round)
    ctx.strokePath()
    ctx.restoreGState()
    // The two halves. The shared shadow above replaces their own.
    for (sign, face, side) in [(CGFloat(-1), blueFace, blueSide), (CGFloat(1), redFace, redSide)] {
        let half = CGMutablePath()
        half.move(to: CGPoint(x: sign * 2000, y: top + 2000))
        half.addLine(to: CGPoint(x: crack[0].x, y: top + 2000))
        // addLines(between:) would start a new subpath, so add each point to this one.
        for point in crack { half.addLine(to: point) }
        half.addLine(to: CGPoint(x: crack[crack.count - 1].x, y: bottom - 2000))
        half.addLine(to: CGPoint(x: sign * 2000, y: bottom - 2000))
        half.closeSubpath()
        ctx.saveGState()
        ctx.concatenate(place(sign))
        ctx.addPath(half)
        ctx.clip()
        keycap(ctx, center: .zero, w: w, depth: depth, radius: radius, face: face, side: side, shadowAlpha: 0) { c in
            glyph(c, "⌘", at: .zero, size: 250, color: rgb(0xFFFFFF), shadow: rgb(0x000000, 0.25))
        }
        ctx.restoreGState()
    }
    if spark {
        let mouth = CGPoint(x: 0, y: top + 6)
        sparks(ctx, from: mouth, color: light ? 0xF2A23A : 0xFFE08A,
               [(58.0, 44.0, 92.0, 14.0), (90.0, 50.0, 106.0, 15.0), (122.0, 44.0, 92.0, 14.0)])
    }
    ctx.restoreGState()
}

/// B. Two keys seen from above that overlap. The shared area is the clash.
func overlap(_ ctx: CGContext) {
    drawBody(ctx, top: 0xFFFFFF, bottom: 0xE1E5EC, edge: 0.9)
    let s: CGFloat = 388, r: CGFloat = 104, off: CGFloat = 104
    let blue = rounded(CGRect(x: 512 - off - s / 2, y: 512 + off - s / 2, width: s, height: s), r)
    let red = rounded(CGRect(x: 512 + off - s / 2, y: 512 - off - s / 2, width: s, height: s), r)
    for (path, colors) in [(blue, blueFace), (red, redFace)] {
        ctx.saveGState()
        ctx.setShadow(offset: CGSize(width: 0, height: -14), blur: 30, color: rgb(0x0B1530, 0.28))
        ctx.addPath(path)
        ctx.setFillColor(rgb(colors.1))
        ctx.fillPath()
        ctx.restoreGState()
        ctx.saveGState()
        ctx.addPath(path)
        ctx.clip()
        let box = path.boundingBox
        ctx.drawLinearGradient(grad([rgb(colors.0), rgb(colors.1)]), start: CGPoint(x: 0, y: box.maxY), end: CGPoint(x: 0, y: box.minY), options: [])
        ctx.restoreGState()
    }
    // The shared area.
    ctx.saveGState()
    ctx.addPath(blue)
    ctx.clip()
    ctx.addPath(red)
    ctx.clip()
    ctx.drawLinearGradient(grad([rgb(0xFFE07A), rgb(0xFFA62E)]), start: CGPoint(x: 0, y: 512 + 92), end: CGPoint(x: 0, y: 512 - 92), options: [])
    ctx.restoreGState()
    glyph(ctx, "⌘", at: CGPoint(x: 512, y: 512), size: 124, weight: .bold, color: rgb(0x2A1A05))
    // Faint legends in the parts each key owns alone.
    glyph(ctx, "⌘", at: CGPoint(x: 512 - off - 70, y: 512 + off + 70), size: 96, color: rgb(0xFFFFFF, 0.92))
    glyph(ctx, "⌘", at: CGPoint(x: 512 + off + 70, y: 512 - off - 70), size: 96, color: rgb(0xFFFFFF, 0.92))
}

/// C. One white key with its legend printed twice, out of register: blue and red want the same key.
func doublePrint(_ ctx: CGContext) {
    drawBody(ctx, top: 0xEEF1F5, bottom: 0xCDD3DD, edge: 0.9)
    keycap(ctx, center: CGPoint(x: 512, y: 540), w: 500, depth: 48, radius: 112, face: (0xFFFFFF, 0xEBEEF2),
           side: 0xB9C0CC, rim: rgb(0xFFFFFF, 0.9), shadowAlpha: 0.30) { c in
        c.setBlendMode(.multiply)
        glyph(c, "⌘", at: CGPoint(x: -16, y: 10), size: 290, weight: .semibold, color: rgb(0x2F7BF5))
        glyph(c, "⌘", at: CGPoint(x: 16, y: -10), size: 290, weight: .semibold, color: rgb(0xF0444F))
    }
}

/// D. A block of dark keys with one lit red: find the clash.
func spotTheClash(_ ctx: CGContext) {
    let k: CGFloat = 196, gapK: CGFloat = 34, depth: CGFloat = 24
    let step = k + gapK
    let lift: CGFloat = 12
    drawBody(ctx, top: 0x2B3145, bottom: 0x0F1219) { c in
        glow(c, at: CGPoint(x: 512, y: 512 + lift), radius: 400, color: 0xFF4D57, alpha: 0.8)
    }
    let legends = [["⌃", "⌥", "⇧"], ["⇥", "⌘", "⌫"], ["⎋", "↩", "⇪"]]
    for row in 0..<3 {
        for col in 0..<3 {
            let center = CGPoint(x: 512 + CGFloat(col - 1) * step, y: 512 + lift + CGFloat(1 - row) * step)
            let hot = row == 1 && col == 1
            keycap(ctx, center: center, w: k, depth: depth, radius: 46,
                   face: hot ? redFace : (0x3B4252, 0x2A2F3B), side: hot ? redSide : 0x171A21,
                   rim: rgb(0xFFFFFF, hot ? 0.30 : 0.08), shadowAlpha: 0.55) { c in
                glyph(c, legends[row][col], at: .zero, size: hot ? 118 : 84, weight: hot ? .semibold : .medium,
                      color: hot ? rgb(0xFFFFFF) : rgb(0x8E95A3), shadow: hot ? rgb(0x000000, 0.25) : nil)
            }
        }
    }
}

/// E. A K for Keyclash: the two arms, blue and red, meet the stem at one point and spark.
func monogram(_ ctx: CGContext) {
    drawBody(ctx, top: 0x262D40, bottom: 0x0E1118) { c in
        glow(c, at: CGPoint(x: 430, y: 512), radius: 420, color: 0xFFB13D, alpha: 0.28)
    }
    let width: CGFloat = 112
    let junction = CGPoint(x: 418, y: 512)
    bar(ctx, from: CGPoint(x: 362, y: 286), to: CGPoint(x: 362, y: 738), width: width, colors: (0xFFFFFF, 0xD3DAE6))
    bar(ctx, from: junction, to: CGPoint(x: 668, y: 738), width: width, colors: blueFace)
    bar(ctx, from: junction, to: CGPoint(x: 668, y: 286), width: width, colors: redFace)
    drawBurst(ctx, at: CGPoint(x: junction.x + 8, y: junction.y), size: 104)
    sparks(ctx, from: CGPoint(x: junction.x + 8, y: junction.y), color: 0xFFE08A,
           [(-18.0, 142.0, 178.0, 13.0), (0.0, 148.0, 192.0, 15.0), (18.0, 142.0, 178.0, 13.0)])
}

/// F. A frosted glass key in front of a blue light and a red light, in the style of current macOS.
func glassKey(_ ctx: CGContext) {
    drawBody(ctx, top: 0x1B2133, bottom: 0x090B11) { c in
        glow(c, at: CGPoint(x: 360, y: 640), radius: 480, color: 0x2F86FF, alpha: 1.0)
        c.setBlendMode(.screen)
        glow(c, at: CGPoint(x: 668, y: 392), radius: 480, color: 0xFF3D52, alpha: 1.0)
        c.setBlendMode(.normal)
    }
    let key = rounded(CGRect(x: 512 - 260, y: 512 - 260, width: 520, height: 520), 128)
    // A shadow outside the glass only, so the glass stays clear.
    ctx.saveGState()
    ctx.addRect(CGRect(x: 0, y: 0, width: 1024, height: 1024))
    ctx.addPath(key)
    ctx.clip(using: .evenOdd)
    ctx.setShadow(offset: CGSize(width: 0, height: -22), blur: 50, color: rgb(0x000000, 0.45))
    ctx.addPath(key)
    ctx.setFillColor(rgb(0x000000))
    ctx.fillPath()
    ctx.restoreGState()
    // The glass.
    ctx.saveGState()
    ctx.addPath(key)
    ctx.clip()
    ctx.setFillColor(rgb(0xFFFFFF, 0.12))
    ctx.fill(CGRect(x: 0, y: 0, width: 1024, height: 1024))
    ctx.drawLinearGradient(grad([rgb(0xFFFFFF, 0.34), rgb(0xFFFFFF, 0.06), rgb(0xFFFFFF, 0.0)], [0, 0.55, 1]),
                           start: CGPoint(x: 0, y: 772), end: CGPoint(x: 0, y: 252), options: [])
    ctx.restoreGState()
    // The rim: bright at the top left, faint at the bottom right.
    ctx.saveGState()
    ctx.addPath(rounded(CGRect(x: 512 - 257, y: 512 - 257, width: 514, height: 514), 125))
    ctx.setLineWidth(6)
    ctx.replacePathWithStrokedPath()
    ctx.clip()
    ctx.drawLinearGradient(grad([rgb(0xFFFFFF, 0.95), rgb(0xFFFFFF, 0.25), rgb(0xFFFFFF, 0.55)], [0, 0.6, 1]),
                           start: CGPoint(x: 252, y: 772), end: CGPoint(x: 772, y: 252), options: [])
    ctx.restoreGState()
    glyph(ctx, "⌘", at: CGPoint(x: 512, y: 512), size: 264, weight: .medium, color: rgb(0xFFFFFF),
          shadow: rgb(0xFFFFFF, 0.55), shadowBlur: 26, shadowY: 0)
}

/// G. The current icon on a light body, for a light Dock and the light app appearance.
func daylight(_ ctx: CGContext) {
    let hit = CGPoint(x: 512, y: 606)
    drawBody(ctx, top: 0xFFFFFF, bottom: 0xDFE4EB, edge: 0.9) { c in
        glow(c, at: hit, radius: 430, color: 0xFFB13D, alpha: 0.32)
    }
    let lean: CGFloat = 0.21
    keycap(ctx, center: CGPoint(x: 344, y: 432), angle: -lean, w: 318, depth: 36, radius: 70, face: blueFace, side: blueSide, shadowAlpha: 0.30) { c in
        glyph(c, "⌘", at: .zero, size: 188, color: rgb(0xFFFFFF), shadow: rgb(0x000000, 0.22))
    }
    keycap(ctx, center: CGPoint(x: 680, y: 432), angle: lean, w: 318, depth: 36, radius: 70, face: redFace, side: redSide, shadowAlpha: 0.30) { c in
        glyph(c, "⌘", at: .zero, size: 188, color: rgb(0xFFFFFF), shadow: rgb(0x000000, 0.22))
    }
    drawBurst(ctx, at: hit, size: 150)
    sparks(ctx, from: hit, color: 0xF2A23A,
           [(34.0, 186.0, 222.0, 13.0), (62.0, 190.0, 232.0, 15.0), (90.0, 192.0, 228.0, 15.0), (118.0, 190.0, 232.0, 15.0), (146.0, 186.0, 222.0, 13.0)])
}

let concepts: [(String, (CGContext) -> Void)] = [
    ("a-split-key", { splitKey($0) }),
    ("a2-split-key-break", { splitKey($0, tilt: 0.05, spark: true) }),
    ("a3-split-key-light", { splitKey($0, light: true) }),
    ("a4-split-key-clean", { splitKey($0, zigzag: false) }),
    ("b-overlap", overlap),
    ("c-double-print", doublePrint),
    ("d-spot-the-clash", spotTheClash),
    ("e-monogram", monogram),
    ("f-glass-key", glassKey),
    ("g-daylight", daylight),
]
do {
    try FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)
} catch {
    FileHandle.standardError.write("cannot create \(outDir): \(error.localizedDescription)\n".data(using: .utf8)!)
    exit(1)
}
for (name, draw) in concepts {
    let ctx = newContext()
    draw(ctx)
    save(ctx, "\(outDir)/\(name).png")
}
