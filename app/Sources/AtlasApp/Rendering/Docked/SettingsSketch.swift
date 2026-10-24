#if DEBUG
  import AtlasUI
  import SwiftUI

  /// A sample System Settings window for the docked renders: a sidebar of bars, the page's name, and a list of apps
  /// with their switches, drawn plainly so it reads as the place the panel docks beside, never as the real window.
  struct SettingsSketch: View {
    /// The page's name, as macOS names it.
    let page: String

    /// The window: its sidebar, then its page.
    var body: some View {
      HStack(spacing: 0) {
        VStack(alignment: .leading, spacing: 10) {
          ForEach(0..<10, id: \.self) { index in
            RoundedRectangle(cornerRadius: 4)
              .fill(Color.white.opacity(index == 6 ? 0.22 : 0.08))
              .frame(width: index.isMultiple(of: 3) ? 120 : 150, height: 14)
          }
          Spacer()
        }
        .padding(.top, 52)
        .padding(.horizontal, 16)
        .frame(width: 200)
        .frame(maxHeight: .infinity)
        .background(Color.white.opacity(0.04))
        VStack(alignment: .leading, spacing: 16) {
          Text(page)
            .font(.system(size: 15, weight: .bold))
            .foregroundStyle(Color.white.opacity(0.85))
          VStack(spacing: 0) {
            ForEach(0..<3, id: \.self) { _ in
              HStack(spacing: 10) {
                RoundedRectangle(cornerRadius: 5)
                  .fill(Color.white.opacity(0.14))
                  .frame(width: 22, height: 22)
                RoundedRectangle(cornerRadius: 3)
                  .fill(Color.white.opacity(0.12))
                  .frame(width: 110, height: 10)
                Spacer()
                Capsule()
                  .fill(Color.white.opacity(0.12))
                  .frame(width: 32, height: 18)
              }
              .padding(.horizontal, 12)
              .frame(height: 40)
            }
          }
          .background(RoundedRectangle(cornerRadius: 10).fill(Color.white.opacity(0.05)))
          Spacer()
        }
        .padding(.top, 48)
        .padding(.horizontal, 22)
      }
      .background(Color(white: 0.16))
      .clipShape(RoundedRectangle(cornerRadius: 12))
      .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Color.white.opacity(0.12), lineWidth: 0.5))
    }
  }
#endif
