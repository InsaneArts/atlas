#if DEBUG
  import AtlasUI
  import SwiftUI

  /// The main window's content for a transition's key frame, as the renders draw it: the pose's first state until the
  /// change, then its second, taken in the animation the change runs in (``ViewerMotionPose/animation``), as the view
  /// wraps the person's own change, so the frame shows what the window shows at that instant.
  struct ViewerMotionFrame: View {
    /// The transition.
    let pose: ViewerMotionPose
    /// Whether the change has been made.
    let isChanged: Bool
    /// What the window shows.
    @State private var shown: ViewerState

    /// The content of `pose`, before or after its change.
    ///
    /// - Parameters:
    ///   - pose: The transition.
    ///   - isChanged: Whether the change has been made.
    init(_ pose: ViewerMotionPose, isChanged: Bool) {
      self.pose = pose
      self.isChanged = isChanged
      _shown = State(initialValue: isChanged ? pose.to : pose.from)
    }

    /// The window, with the sample icons; arrivals play, since a key frame is drawn part of the way through them.
    var body: some View {
      ViewerView(state: shown) { _ in }
        .environment(\.appIcon, AppIconImage.image)
        .onChange(of: isChanged) {
          withAnimation(pose.animation()) {
            shown = isChanged ? pose.to : pose.from
          }
        }
    }
  }
#endif
