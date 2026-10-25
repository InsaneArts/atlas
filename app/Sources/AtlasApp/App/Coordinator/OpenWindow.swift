/// An open window and the controller it shows, as the coordinator keeps them: the window to bring forward or close,
/// the controller to hand what another window asks.
struct OpenWindow<Controller: AnyObject> {
  /// The window.
  let window: any WindowPresenting
  /// Its controller.
  let controller: Controller
}
