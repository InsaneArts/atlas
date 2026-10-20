extension PanelState {
  /// How Open Viewer is painted: white as the next step, glass beside the way to log in.
  var viewerLook: CapsuleButtonStyle.Look {
    isViewerPrimary ? .primary : .glass
  }
}
