#if DEBUG
  import SwiftUI
  import Testing

  @testable import AtlasUI

  @MainActor
  @Suite("OnboardingView")
  struct OnboardingViewTests {
    @Test("the footer stays in place while the screen above changes")
    func footerOutsideTheScreens() throws {
      let view = OnboardingView(state: PreviewStates.onboardingChoose, form: .constant(PreviewStates.onboardingForm)) {
        _ in
      }
      // The body's type spells out the view's tree. A new screen is a new identity inside the stage, so the footer,
      // drawn beside the stage and not within its generic arguments, is never replaced or slid with a screen.
      let tree = String(reflecting: type(of: view.body))
      let stage = try #require(tree.range(of: "PageStage<"), "the screens slide on a page stage")
      let stageEnd = try #require(Self.closingBracket(in: tree, after: stage.upperBound))
      let footer = try #require(tree.range(of: "OnboardingFooter"), "the window draws a footer")
      #expect(!tree[stage.upperBound..<stageEnd].contains("OnboardingFooter"), "the footer is not part of a screen")
      #expect(footer.lowerBound > stageEnd, "the footer comes after the stage, under it")
    }

    /// Where the generic arguments opened just before `start` close.
    ///
    /// - Parameters:
    ///   - text: A type's name.
    ///   - start: Just past an opening `<`.
    /// - Returns: The index of its matching `>`, or nil when it never closes.
    private static func closingBracket(in text: String, after start: String.Index) -> String.Index? {
      var depth = 1
      var index = start
      while index < text.endIndex {
        switch text[index] {
        case "<": depth += 1
        case ">":
          depth -= 1
          if depth == 0 { return index }
        default: break
        }
        index = text.index(after: index)
      }
      return nil
    }
  }
#endif
