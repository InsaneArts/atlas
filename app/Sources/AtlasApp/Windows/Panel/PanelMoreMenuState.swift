/// The panel's ⋯ menu as the presenter decides it: whether Open Atlas leads it.
struct PanelMoreMenuState: Equatable {
  /// Whether Open Atlas leads the menu: only while the panel's main button is something else.
  var showsOpenAtlas: Bool
}
