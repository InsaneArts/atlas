/// The panel's ⋯ menu as the presenter decides it: which of the helper's commands it offers, and Open at Login.
struct PanelMoreMenuState: Equatable {
  /// Which of the helper's commands it offers.
  var helper: HelperMenuState
  /// Open at Login's tick; nil leaves the item out, when the app cannot be a login item.
  var opensAtLogin: Bool?
}
