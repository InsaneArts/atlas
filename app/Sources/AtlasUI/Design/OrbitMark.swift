import SwiftUI

/// Atlas's orbit at any size, for headers: the same shape as the menu
/// bar glyph, drawn as vectors, in the colour of the text around it.
struct OrbitMark: View {
    var size: CGFloat = 22

    var body: some View {
        Canvas { context, canvas in
            let scale = canvas.width / 18
            let center = CGPoint(x: 9 * scale, y: 8 * scale)
            let line = max(1.3, 1.25 * scale)
            let tilt = CGAffineTransform(translationX: center.x, y: center.y)
                .rotated(by: -20 * .pi / 180)
                .scaledBy(x: 8.2 * scale, y: 2.5 * scale)
            /// Part of the ring, by angle; 0 to 180 degrees is the half in front.
            func ring(_ start: Double, _ end: Double) -> Path {
                var path = Path()
                let steps = 96
                for step in 0...steps {
                    let angle = (start + (end - start) * Double(step) / Double(steps)) * .pi / 180
                    let point = CGPoint(x: cos(angle), y: sin(angle)).applying(tilt)
                    if step == 0 { path.move(to: point) } else { path.addLine(to: point) }
                }
                return path
            }
            let radius = 4.6 * scale
            let sphere = Path(
                ellipseIn: CGRect(x: center.x - radius, y: center.y - radius, width: 2 * radius, height: 2 * radius))
            // One layer, so overlapping strokes never darken and the gap
            // where the ring passes in front can be cleared.
            context.drawLayer { layer in
                layer.stroke(ring(0, 360), with: .foreground, style: StrokeStyle(lineWidth: line))
                layer.fill(sphere, with: .foreground)
                layer.blendMode = .destinationOut
                layer.stroke(ring(40, 140), with: .foreground, style: StrokeStyle(lineWidth: line + 1.3 * scale))
                layer.blendMode = .normal
                // The sphere again below the ring, so the gap shows only above it.
                layer.drawLayer { below in
                    below.clip(to: sphere)
                    var lower = ring(0, 180)
                    lower.addLine(to: CGPoint(x: -1, y: 10).applying(tilt))
                    lower.addLine(to: CGPoint(x: 1, y: 10).applying(tilt))
                    lower.closeSubpath()
                    below.fill(lower, with: .foreground)
                }
                layer.stroke(ring(0, 180), with: .foreground, style: StrokeStyle(lineWidth: line))
            }
        }
        .frame(width: size, height: size * 16 / 18)
        .accessibilityHidden(true)
    }
}
