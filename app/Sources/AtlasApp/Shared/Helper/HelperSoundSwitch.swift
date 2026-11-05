import AtlasCommands
import AtlasService
import Observation

/// The helper's sound as every window shows it, the menu bar panel, the viewer and Settings alike, and the one way
/// any of them changes it. One `atlas sound` runs at a time: a choice made while one runs waits for it, then only the
/// latest is sent, and only its answer is shown. The switch shows the choice at once and never flips back on a
/// failure; what went wrong is its caption. ``HelperActions`` holds the one instance every window shares.
@MainActor
@Observable
final class HelperSoundSwitch {
  /// What the helper last said of its sound, or the choice just made; nil until it says.
  private(set) var sound: HelperSound?
  /// Why the last change or reading did not reach the helper; nil once one did.
  private var failure: String?
  /// The `atlas` command line.
  @ObservationIgnored private let commands: any CommandRunning
  /// Numbers each reading or change, so a reading, or a status asked for, older than a change is dropped.
  @ObservationIgnored private(set) var generation = 0
  /// The choice made last that `atlas sound` has not been run for yet.
  @ObservationIgnored private var wanted: Bool?
  /// Whether `atlas sound` is changing the helper's sound now.
  @ObservationIgnored private(set) var sending = false
  /// Whether the last change did not reach the helper, so its choice stays shown over what a status says.
  @ObservationIgnored private var changeFailed = false

  /// Creates the switch.
  ///
  /// - Parameter commands: The `atlas` command line.
  init(commands: any CommandRunning) {
    self.commands = commands
  }

  /// Whether the person wants apps in the helper silent: the helper's saved choice, silent until it says.
  var isMuted: Bool { sound?.wanted ?? true }

  /// Why the sound is not as chosen, or why the last change did not reach the helper, with its fix; nil when all is
  /// well.
  var problem: String? { failure ?? sound?.problem }

  /// Takes what a status reading says of the helper's sound, unless a change or reading came since the status was
  /// asked for, whose answer wins, or the last change failed, whose choice stays shown with why until the helper
  /// answers again.
  ///
  /// - Parameters:
  ///   - said: The sound the status carries; nil when it says none.
  ///   - asked: ``generation`` when the status was asked for.
  func adopt(_ said: HelperSound?, asked: Int) {
    guard let said, asked == generation, !sending, wanted == nil, !changeFailed else { return }
    sound = said
    failure = nil
  }

  /// Reads whether apps in the helper can be heard. None starts while a change runs, and a change started meanwhile
  /// wins over its answer. A reading that fails says why, such as an older helper that must restart first.
  func read() async {
    guard !sending else { return }
    generation += 1
    let number = generation
    let outcome = await commands.run(.helperSound)
    guard number == generation, !sending else { return }
    if let said = HelperSound.of(outcome) {
      sound = said
      failure = nil
      // The helper answered for itself: a change that did not reach it no longer stands over its status.
      changeFailed = false
    } else {
      failure = outcome.message
    }
  }

  /// Turns the helper's sound the other way from the saved choice: the panel's and the main window's button.
  func toggle() async {
    await set(muted: !isMuted)
  }

  /// Keeps apps in the helper silent, or lets them play.
  ///
  /// - Parameter muted: Whether apps in the helper stay silent.
  func set(muted: Bool) async {
    generation += 1
    sound = HelperSound(state: muted ? .muted : .on, wanted: muted)
    failure = nil
    wanted = muted
    guard !sending else { return }
    sending = true
    defer {
      sending = false
      generation += 1
    }
    while let next = wanted {
      wanted = nil
      let outcome = await commands.run(.setHelperSound(muted: next))
      guard wanted == nil else { continue }
      if let said = HelperSound.of(outcome) {
        sound = said
        changeFailed = false
      } else {
        failure = outcome.message
        changeFailed = true
      }
    }
  }
}
