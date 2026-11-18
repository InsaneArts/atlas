import AppKit
import AtlasVMKit
import Virtualization

/// A VM's window, with its input: shown only while the person's hands are
/// on the VM.
@MainActor
final class VMWindow {
  private let window: NSWindow
  private let machine: VZVirtualMachine

  /// A window on `machine`, shown at once.
  init(machine: VZVirtualMachine, title: String) {
    self.machine = machine
    let view = VZVirtualMachineView()
    view.virtualMachine = machine
    view.capturesSystemKeys = true
    view.automaticallyReconfiguresDisplay = true
    window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 1280, height: 800),
      styleMask: [.titled, .closable, .miniaturizable, .resizable],
      backing: .buffered,
      defer: false
    )
    window.title = title
    window.isReleasedWhenClosed = false
    window.contentView = view
    window.center()
    window.makeKeyAndOrderFront(nil)
    NSApplication.shared
      .activate()
  }

  /// Whether it shows `machine`.
  func shows(_ machine: VZVirtualMachine) -> Bool { self.machine === machine }

  /// Whether it is open: on screen, or minimized, from where the person
  /// restores it with its input. Only its close makes it closed.
  var isOpen: Bool {
    WindowState(visible: window.isVisible, minimized: window.isMiniaturized) == .open
  }

  /// Close it.
  func close() {
    window.close()
  }
}
