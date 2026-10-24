import SwiftUI

extension EnvironmentValues {
  /// Which way the pages under a ``PageStage`` move now. The stage sets it above its pages, so the page leaving reads
  /// the new direction too: a transition keeps the values it had when its page last drew, and the environment is the
  /// one input a leaving page still reads.
  @Entry var pageDirection = PageDirection.forward
}
