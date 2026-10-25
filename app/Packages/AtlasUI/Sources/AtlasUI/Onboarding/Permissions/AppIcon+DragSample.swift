#if DEBUG
  extension AppIcon {
    /// Atlas's icon at the permission panel's 64-point tile, still: what previews and renders draw in place of the
    /// draggable icon the app hands in.
    public static var dragSample: AppIcon {
      AppIcon(tile: OnboardingLayout.dragTile)
    }
  }
#endif
