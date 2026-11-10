#if DEBUG
  import CoreGraphics
  import Foundation

  extension PreviewStates {
    /// The samples' screenshot file names, newest first: 10:31:45, 10:31:44, 10:31:20, 10:31:14, 10:12:03 and
    /// 09:58:41 on 22 October 2026, in UTC.
    public static let shotNames = [
      "1792665105000000000-capture.png",
      "1792665104000000000-capture.png",
      "1792665080000000000-capture.png",
      "1792665074000000000-capture.png",
      "1792663923000000000-capture.png",
      "1792663121000000000-capture.png",
    ]

    /// The pictures taken before the samples' typing and key press in Opie, at 10:31:17 and 10:31:15 on 22 October
    /// 2026, in UTC.
    public static let beforeNames = [
      "1792665077000000000-before.jpg",
      "1792665075000000000-before.jpg",
    ]

    /// A screenshot's thumbnail in the samples: the sample desktop at the samples' 2560 by 1440 captures' shape,
    /// known by the screenshot's name.
    ///
    /// - Parameter name: The screenshot's file name.
    /// - Returns: The thumbnail, or nil should the sample desktop not draw.
    @MainActor public static func shotPicture(_ name: String) -> DesktopPicture? {
      SampleDesktop.thumbnail(name, display: shotDisplay)
    }

    /// 2560 by 1440: the samples' captures' size, as the screenshots list says it.
    public static let shotDisplay = CGSize(width: 2560, height: 1440)

    /// 1512 by 982: the helper's display in the samples, a MacBook Air's.
    static let helperDisplay = CGSize(width: 1512, height: 982)

    /// 672 by 438: a window capture's size in the samples.
    static let windowCapture = CGSize(width: 672, height: 438)
  }
#endif
