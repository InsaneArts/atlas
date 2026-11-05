#if DEBUG
  import SwiftUI

  /// A sample sign-in page in a browser on the helper's desktop, as the picture before an agent types a password
  /// shows it: an email field filled in, a password field showing dots, and the button. It stands in the same place
  /// as ``SampleChatScene``'s window, whose ``SampleChatScene/frame(left:top:width:height:)`` names where its parts
  /// stand. Its colours, sizes and words are a sample page's, not the app's, so they are not tokens.
  struct SampleSignInScene: View {
    /// The display's size, in points.
    var display = CGSize(width: 1440, height: 900)

    /// The window on the desktop's ground.
    var body: some View {
      ZStack(alignment: .topLeading) {
        LinearGradient(
          colors: [Color(red: 0.56, green: 0.66, blue: 0.79), Color(red: 0.31, green: 0.42, blue: 0.58)],
          startPoint: .topLeading,
          endPoint: .bottomTrailing
        )
        page
          .frame(width: size.width, height: size.height)
          .clipShape(RoundedRectangle(cornerRadius: unit * 1.1, style: .continuous))
          .shadow(color: .black.opacity(0.28), radius: unit * 2, y: unit * 1.4)
          .offset(
            x: display.width * SampleChatScene.window.minX,
            y: display.height * SampleChatScene.window.minY
          )
      }
      .frame(width: display.width, height: display.height)
      .environment(\.colorScheme, .light)
    }

    /// One hundredth of the display's width, which the page's type and corners are measured in.
    private var unit: CGFloat { display.width / 100 }

    /// The window's size.
    private var size: CGSize {
      CGSize(
        width: display.width * SampleChatScene.window.width,
        height: display.height * SampleChatScene.window.height
      )
    }

    /// The page: the browser's bars, the heading, the two fields and the button.
    private var page: some View {
      ZStack(alignment: .topLeading) {
        Color.white
        Color(red: 0.93, green: 0.93, blue: 0.93)
          .frame(width: size.width, height: size.height * 0.07)
        HStack(spacing: unit * 0.7) {
          ForEach(
            [Color(red: 1, green: 0.37, blue: 0.34), Color(red: 1, green: 0.74, blue: 0.18), Color.green],
            id: \.self
          ) { colour in
            Circle()
              .fill(colour)
              .frame(width: unit * 1.1, height: unit * 1.1)
          }
          text("Sign in", size: 1.3, colour: quiet)
            .padding(.leading, unit * 1.3)
        }
        .frame(height: size.height * 0.07)
        .padding(.leading, unit * 1.4)
        text("accounts.example.com/sign-in", size: 1.25, colour: quiet)
          .padding(.horizontal, unit * 1.4)
          .padding(.vertical, unit * 0.3)
          .background(Capsule().fill(Color.white))
          .frame(width: size.width * 0.9, height: size.height * 0.07, alignment: .leading)
          .padding(.leading, size.width * 0.1)
          .background(Color(red: 0.95, green: 0.95, blue: 0.96))
          .offset(y: size.height * 0.07)
        text("Sign in", size: 2.2, weight: .bold, colour: ink)
          .frame(width: size.width)
          .offset(y: size.height * 0.23)
        text("Email", size: 1.25, colour: quiet)
          .offset(x: size.width * 0.34, y: size.height * 0.34)
        field("sam@example.com", top: 0.39)
        text("Password", size: 1.25, colour: quiet)
          .offset(x: size.width * 0.34, y: size.height * 0.51)
        field("••••••••••", top: 0.56)
        text("Sign in", size: 1.4, weight: .semibold, colour: .white)
          .frame(width: size.width * 0.32, height: size.height * 0.07)
          .background(
            RoundedRectangle(cornerRadius: unit * 0.6, style: .continuous)
              .fill(Color(red: 0.12, green: 0.53, blue: 0.24))
          )
          .offset(x: size.width * 0.34, y: size.height * 0.7)
      }
    }

    /// A filled-in field.
    ///
    /// - Parameters:
    ///   - words: What it shows.
    ///   - top: Its top edge, as a share of the window's height.
    /// - Returns: The field.
    private func field(_ words: String, top: CGFloat) -> some View {
      text(words, size: 1.4, colour: ink)
        .padding(.leading, size.width * 0.012)
        .frame(width: size.width * 0.32, height: size.height * 0.07, alignment: .leading)
        .overlay {
          RoundedRectangle(cornerRadius: unit * 0.6, style: .continuous)
            .strokeBorder(Color.black.opacity(0.22), lineWidth: 1.5)
        }
        .offset(x: size.width * 0.34, y: size.height * top)
    }

    /// The page's text colour.
    private var ink: Color { Color(red: 0.11, green: 0.11, blue: 0.11) }

    /// The page's quiet text colour.
    private var quiet: Color { Color(red: 0.38, green: 0.38, blue: 0.38) }

    /// Words at a size measured in hundredths of the display's width.
    ///
    /// - Parameters:
    ///   - words: The words.
    ///   - size: Their size, in hundredths of the display's width.
    ///   - weight: Their weight; regular by default.
    ///   - colour: Their colour.
    /// - Returns: The text.
    private func text(_ words: String, size: CGFloat, weight: Font.Weight = .regular, colour: Color) -> some View {
      Text(verbatim: words)
        .font(.system(size: unit * size, weight: weight))
        .foregroundStyle(colour)
        .lineLimit(1)
    }
  }
#endif
