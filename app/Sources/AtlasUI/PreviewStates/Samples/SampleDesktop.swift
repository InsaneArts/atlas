#if DEBUG
  import SwiftUI

  /// A stand-in desktop for the panel's and the viewer's samples: a window on a blue ground, drawn once at a
  /// display's size, one pixel a point. Its colours, sizes and words are a sample desktop's, not the app's, so they
  /// are not tokens.
  package enum SampleDesktop {
    /// The desktop as a picture, or nil should drawing it fail.
    @MainActor package static let picture: DesktopPicture? = {
      let renderer = ImageRenderer(content: SampleDesktopScene())
      renderer.scale = 1
      return renderer.cgImage.map { DesktopPicture(id: "sample-desktop", image: $0) }
    }()
  }

  /// The sample desktop's drawing: a menu bar strip over a blue ground, and one window with its title bar, its
  /// three buttons and a few lines of text.
  private struct SampleDesktopScene: View {
    /// A laptop display's size, in points.
    private let display = CGSize(width: 1512, height: 982)

    var body: some View {
      ZStack(alignment: .topLeading) {
        LinearGradient(
          colors: [Color(red: 0.49, green: 0.59, blue: 0.72), Color(red: 0.2, green: 0.31, blue: 0.44)],
          startPoint: .top,
          endPoint: .bottom
        )
        Color.white
          .opacity(0.38)
          .frame(height: display.height * 0.032)
        window
          .frame(width: display.width * 0.56, height: display.height * 0.7)
          .offset(x: display.width * 0.16, y: display.height * 0.12)
      }
      .frame(width: display.width, height: display.height)
      .environment(\.colorScheme, .light)
    }

    /// A white window with a pale title bar, its three buttons, and an app's name over a line about it.
    private var window: some View {
      VStack(alignment: .leading, spacing: 0) {
        HStack(spacing: 7) {
          Circle()
            .fill(Color.red)
            .frame(width: 11, height: 11)
          Circle()
            .fill(Color.yellow)
            .frame(width: 11, height: 11)
          Circle()
            .fill(Color.green)
            .frame(width: 11, height: 11)
        }
        .padding(.leading, 14)
        .frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
        .background(Color(white: 0.965))
        Text(verbatim: "Atlas 0.1.0\n\nAgents test apps in a hidden helper login while you keep working.")
          .font(.system(size: 22))
          .foregroundStyle(Color.black)
          .padding(.horizontal, 40)
          .padding(.top, 42)
        Spacer(minLength: 0)
      }
      .background(Color.white)
      .clipShape(RoundedRectangle(cornerRadius: 10))
    }
  }
#endif
