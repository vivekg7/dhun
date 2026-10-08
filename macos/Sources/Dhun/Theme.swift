import DhunKit
import SwiftUI

/// The phone's eight accent palettes (plan 011), Dull Orange first, with a
/// lighter shade for dark mode. The Mac follows the system's light or dark
/// appearance unless one is chosen; the phone's Black and by-time themes
/// solve phone problems (plan 024).
struct Palette: Identifiable {
    let name: String
    let light: UInt32
    let dark: UInt32
    var id: String { name }

    static let all = [
        Palette(name: "Dull Orange", light: 0xC27B45, dark: 0xD9925C),
        Palette(name: "Sage", light: 0x7E9C7A, dark: 0x98B594),
        Palette(name: "Slate Blue", light: 0x6A7FA8, dark: 0x8C9FC6),
        Palette(name: "Teal", light: 0x4E9A96, dark: 0x6FB6B1),
        Palette(name: "Dusty Rose", light: 0xB9707F, dark: 0xD08D9B),
        Palette(name: "Mauve", light: 0x9479A8, dark: 0xB097C2),
        Palette(name: "Olive", light: 0x8F9152, dark: 0xADAF6F),
        Palette(name: "Sand", light: 0xB59E73, dark: 0xCBB68C),
    ]

    var color: Color {
        Color(
            nsColor: NSColor(name: nil) { a in
                let dark = a.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                return NSColor(hex: dark ? self.dark : self.light)
            })
    }
}

extension NSColor {
    convenience init(hex: UInt32) {
        self.init(
            srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
    }
}

struct Themed: ViewModifier {
    let app: AppModel
    func body(content: Content) -> some View {
        let palette = Palette.all[min(max(app.appearance.palette, 0), Palette.all.count - 1)]
        content
            .tint(palette.color)
            .accentColor(palette.color)
            .preferredColorScheme(
                app.appearance.theme == "light" ? .light : app.appearance.theme == "dark" ? .dark : nil)
    }
}

extension View {
    func themed(_ app: AppModel) -> some View { modifier(Themed(app: app)) }
}

/// "3:07", "1:02:45"
func clock(_ seconds: Double) -> String {
    let t = max(0, Int(seconds.isFinite ? seconds : 0))
    let h = t / 3600
    let m = (t % 3600) / 60
    let s = t % 60
    return h > 0 ? String(format: "%d:%02d:%02d", h, m, s) : String(format: "%d:%02d", m, s)
}
