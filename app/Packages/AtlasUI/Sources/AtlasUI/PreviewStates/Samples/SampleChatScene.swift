#if DEBUG
  import SwiftUI

  /// A sample chat app on the helper's desktop, as the pictures before an agent's actions show it: a workspace's
  /// channels down the side, a channel's messages, and the message box with its send button, at one of the stages of
  /// posting a release note. Every place is a share of the window, so the samples' marks can name where its parts
  /// stand (``frame(left:top:width:height:)``). Its colours, sizes and words are a sample app's, not the app's, so
  /// they are not tokens.
  struct SampleChatScene: View {
    /// Where the task stands.
    var stage = SampleChatStage.empty
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
        chat
          .frame(width: size.width, height: size.height)
          .clipShape(RoundedRectangle(cornerRadius: unit * 1.1, style: .continuous))
          .shadow(color: .black.opacity(0.28), radius: unit * 2, y: unit * 1.4)
          .offset(x: display.width * Self.window.minX, y: display.height * Self.window.minY)
      }
      .frame(width: display.width, height: display.height)
      .environment(\.colorScheme, .light)
    }

    /// The window's place on the display, as shares of it.
    nonisolated static let window = CGRect(x: 0.05, y: 0.06, width: 0.9, height: 0.88)

    /// One hundredth of the display's width, which the window's type and corners are measured in.
    private var unit: CGFloat { display.width / 100 }

    /// The window's size.
    private var size: CGSize {
      CGSize(width: display.width * Self.window.width, height: display.height * Self.window.height)
    }

    /// Where a part of the window stands on the display, as shares of it, from shares of the window.
    ///
    /// - Parameters:
    ///   - left: Its left edge, as a share of the window's width.
    ///   - top: Its top edge, as a share of the window's height.
    ///   - width: Its width, as a share of the window's width.
    ///   - height: Its height, as a share of the window's height.
    /// - Returns: The part's frame, as shares of the display.
    nonisolated static func frame(left: CGFloat, top: CGFloat, width: CGFloat, height: CGFloat) -> CGRect {
      CGRect(
        x: window.minX + left * window.width,
        y: window.minY + top * window.height,
        width: width * window.width,
        height: height * window.height
      )
    }

    /// The window: its bars, the channels, the messages and the message box.
    private var chat: some View {
      ZStack(alignment: .topLeading) {
        Color.white
        Color(red: 0.21, green: 0.05, blue: 0.21)
          .frame(width: size.width, height: size.height * 0.07)
        lights
          .offset(x: unit * 1.4, y: (size.height * 0.07 - unit * 1.1) / 2)
        Color(red: 0.25, green: 0.05, blue: 0.25)
          .frame(width: size.width * 0.22, height: size.height * 0.93)
          .offset(y: size.height * 0.07)
        text("InsaneArts", size: 1.6, weight: .bold, colour: .white)
          .offset(x: size.width * 0.03, y: size.height * 0.105)
        channel("# announcements", top: 0.2, isSelected: stage == .start)
        channel("# general", top: 0.265, isSelected: stage != .start)
        channel("# releases", top: 0.33, isSelected: false)
        channel("# design", top: 0.395, isSelected: false)
        text("Direct messages", size: 1.2, colour: sideText.opacity(0.7))
          .offset(x: size.width * 0.03, y: size.height * 0.49)
        channel("Mia", top: 0.555, isSelected: false)
        channel("Theo", top: 0.62, isSelected: false)
        header
        messages
        box
        send
      }
    }

    /// The window's three buttons.
    private var lights: some View {
      HStack(spacing: unit * 0.7) {
        ForEach(
          [Color(red: 1, green: 0.37, blue: 0.34), Color(red: 1, green: 0.74, blue: 0.18), Color.green],
          id: \.self
        ) { colour in
          Circle()
            .fill(colour)
            .frame(width: unit * 1.1, height: unit * 1.1)
        }
      }
    }

    /// The channels' text colour.
    private var sideText: Color { Color(red: 0.8, green: 0.75, blue: 0.8) }

    /// One channel or person down the side, highlighted when chosen.
    ///
    /// - Parameters:
    ///   - name: Its name.
    ///   - top: Its top edge, as a share of the window's height.
    ///   - isSelected: Whether it is chosen.
    /// - Returns: The item.
    private func channel(_ name: String, top: CGFloat, isSelected: Bool) -> some View {
      text(name, size: 1.4, colour: isSelected ? .white : sideText)
        .padding(.leading, size.width * 0.015)
        .frame(width: size.width * 0.19, height: size.height * 0.055, alignment: .leading)
        .background(
          RoundedRectangle(cornerRadius: unit * 0.6, style: .continuous)
            .fill(isSelected ? Color(red: 0.07, green: 0.39, blue: 0.64) : .clear)
        )
        .offset(x: size.width * 0.015, y: size.height * top)
    }

    /// The channel's name over its messages, ruled off.
    private var header: some View {
      text(stage == .start ? "# announcements" : "# general", size: 1.6, weight: .bold, colour: ink)
        .padding(.leading, size.width * 0.02)
        .frame(width: size.width * 0.78, height: size.height * 0.08, alignment: .leading)
        .overlay(alignment: .bottom) {
          Color(red: 0.89, green: 0.9, blue: 0.92)
            .frame(height: 1)
        }
        .offset(x: size.width * 0.22, y: size.height * 0.07)
    }

    /// The channel's messages: #announcements' one, or #general's two and the release note once posted.
    @ViewBuilder private var messages: some View {
      if stage == .start {
        message("Theo", "The office is closed on Friday.", avatar: Color(red: 0.18, green: 0.71, blue: 0.49), top: 0.19)
      } else {
        message("Mia", "Is 0.1.1 going out today?", avatar: Color(red: 0.91, green: 0.57, blue: 0.18), top: 0.19)
        message(
          "Theo",
          "Tests passed. The build is signed.",
          avatar: Color(red: 0.18, green: 0.71, blue: 0.49),
          top: 0.305
        )
        if stage == .posted {
          message("Sam", Self.releaseNote, avatar: Color(red: 0.21, green: 0.77, blue: 0.94), top: 0.42)
        }
      }
    }

    /// What the agent types and posts.
    nonisolated static let releaseNote = "Release 0.1.1 is out: faster setup and fixes for typing in the helper."

    /// One message: its sender's picture, name and words.
    ///
    /// - Parameters:
    ///   - name: Who sent it.
    ///   - words: What it says.
    ///   - avatar: The sender's picture's colour.
    ///   - top: Its top edge, as a share of the window's height.
    /// - Returns: The message.
    private func message(_ name: String, _ words: String, avatar: Color, top: CGFloat) -> some View {
      HStack(alignment: .top, spacing: unit * 1.2) {
        RoundedRectangle(cornerRadius: unit * 0.7, style: .continuous)
          .fill(avatar)
          .frame(width: unit * 3.4, height: unit * 3.4)
        VStack(alignment: .leading, spacing: unit * 0.2) {
          text(name, size: 1.4, weight: .bold, colour: ink)
          text(words, size: 1.4, colour: Color(red: 0.38, green: 0.38, blue: 0.38))
        }
      }
      .frame(width: size.width * 0.73, alignment: .leading)
      .offset(x: size.width * 0.24, y: size.height * top)
    }

    /// The message box: its placeholder, or the release note once typed.
    private var box: some View {
      ZStack(alignment: .topLeading) {
        RoundedRectangle(cornerRadius: unit, style: .continuous)
          .strokeBorder(Color.black.opacity(0.22), lineWidth: 1.5)
        text(
          stage == .typed ? Self.releaseNote : "Message #general",
          size: 1.4,
          colour: stage == .typed ? ink : Color(red: 0.6, green: 0.6, blue: 0.6)
        )
        .padding(.leading, size.width * 0.016)
        .padding(.top, size.width * 0.012)
        .padding(.trailing, size.width * 0.09)
      }
      .frame(width: size.width * 0.74, height: size.height * 0.15)
      .offset(x: size.width * 0.24, y: size.height * 0.79)
    }

    /// The send button: green once there is something to send.
    private var send: some View {
      Image(systemName: "paperplane")
        .font(.system(size: unit * 1.5, weight: .medium))
        .foregroundStyle(stage == .typed ? Color.white : Color(red: 0.6, green: 0.6, blue: 0.6))
        .frame(width: size.width * 0.044, height: size.height * 0.074)
        .background(
          RoundedRectangle(cornerRadius: unit * 0.6, style: .continuous)
            .fill(stage == .typed ? Color(red: 0, green: 0.48, blue: 0.35) : Color(red: 0.95, green: 0.95, blue: 0.96))
        )
        .offset(x: size.width * 0.924, y: size.height * 0.828)
    }

    /// The app's text colour.
    private var ink: Color { Color(red: 0.11, green: 0.11, blue: 0.11) }

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
        .lineLimit(2)
        .fixedSize(horizontal: false, vertical: true)
    }
  }
#endif
