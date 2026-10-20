#if DEBUG
  import SwiftUI

  /// The rows' samples, one component at a time, at a window's width.
  struct RowSamples: View {
    /// Which component the samples show.
    enum Kind: Sendable {
      /// Waiting, current and done steps.
      case step
      /// Compact and regular facts.
      case fact
      /// Menu rows, one with a shortcut and one disabled.
      case menu
      /// A switch among menu rows, and a picker in a card.
      case control
      /// A selected symbol choice and an unselected letter choice.
      case choice
      /// A plain and a secure field.
      case field
      /// Done, busy and failed checks.
      case check
      /// A symbol tile with a status, and a text tile with a switch.
      case icon
      /// A setting with its switch.
      case setting
      /// The person's, another's and a failed action.
      case activity
      /// Ready, unknown and failing checks.
      case readiness
    }

    private let kind: Kind

    /// Creates the samples of one component.
    ///
    /// - Parameter kind: Which component the samples show.
    init(_ kind: Kind) {
      self.kind = kind
    }

    /// That component's samples, on the ground.
    var body: some View {
      VStack(spacing: Spacing.m) {
        switch kind {
        case .step:
          StepRow(index: 1, title: String("Find Atlas in the list"), detail: String("Privacy & Security"), state: .done)
          StepRow(index: 2, title: String("Turn its switch on"), detail: String("Confirm it"), state: .current)
          StepRow(index: 3, title: String("Come back here"), detail: String("Atlas notices"), state: .waiting)
        case .fact:
          CardGroup {
            FactRow(name: String("Helper"), value: String("Atlas"))
            SeparatorLine()
            FactRow(name: String("Display"), value: String("1512 × 982"), size: .regular)
          }
        case .menu:
          Button(String("Getting Started…")) {}
            .buttonStyle(.menuRow())
          Button(String("Recover")) {}
            .buttonStyle(.menuRow())
            .disabled(true)
          Button(String("Quit Atlas")) {}
            .buttonStyle(.menuRow(shortcut: KeyboardShortcut("q", modifiers: .command)))
        case .control:
          ControlRow(size: .menu) {
            Text(verbatim: "Open at Login")
          } control: {
            Toggle(String("Open at Login"), isOn: .constant(true))
              .toggleStyle(.accentSwitch)
              .controlSize(.mini)
          }
          CardGroup {
            ControlRow {
              Text(verbatim: "Refresh")
                .foregroundStyle(Color.textSecondary)
            } control: {
              Picker(String("Refresh"), selection: .constant(ViewerPictureRate.everyTwoSeconds)) {
                Text(verbatim: "Every 2 seconds")
                  .tag(ViewerPictureRate.everyTwoSeconds)
              }
              .labelsHidden()
              .fixedSize()
            }
          }
        case .choice:
          ChoiceRow(
            title: String("Create a new account"),
            detail: String("Recommended."),
            mark: .symbol("plus"),
            selected: true
          ) {}
          ChoiceRow(
            title: String("Test Account"),
            detail: String("testaccount · standard account on this Mac"),
            mark: .letter("T"),
            selected: false
          ) {}
        case .field:
          CardGroup {
            FieldRow(label: String("Full name"), text: .constant(String("Atlas")))
            SeparatorLine()
            FieldRow(label: String("Password"), text: .constant(String("secret")), secure: true)
          }
        case .check:
          CardGroup {
            CheckRow(text: String("Atlas runs in the helper"), state: .done, note: String("starts by itself"))
            SeparatorLine()
            CheckRow(text: String("A test screenshot"), state: .busy, note: String("checking…"))
            SeparatorLine()
            CheckRow(text: String("A test screenshot"), state: .failed, note: String("failed"))
          }
        case .icon:
          CardGroup {
            IconRow(icon: .symbol("cursorarrow"), title: String("Accessibility"), detail: String("So agents can click"))
            {
              Text(verbatim: "Allowed")
                .font(Typography.font(.callout, weight: .semibold))
                .foregroundStyle(Color.textSuccess)
            }
            SeparatorLine()
            IconRow(icon: .text(">_"), title: String("Codex"), detail: String("Found.")) {
              Toggle(String("Codex"), isOn: .constant(true))
                .toggleStyle(.accentSwitch)
            }
          }
        case .setting:
          CardGroup {
            SettingRow(
              title: String("Open Atlas at login"),
              detail: String("So Atlas is there whenever you are."),
              isOn: .constant(true)
            )
          }
        case .activity:
          CardGroup {
            ActivityRow(initial: String("Y"), line: String("You clicked twice"), when: String("now"), kind: .yours)
            SeparatorLine()
            ActivityRow(initial: String("A"), line: String("Agent ran a command"), when: String("7s"), kind: .others)
            SeparatorLine()
            ActivityRow(initial: String("A"), line: String("Agent typed"), when: String("4m"), kind: .failed)
          }
        case .readiness:
          CardGroup {
            ReadinessRow(name: String("Helper session"), text: String(""), state: .ready)
            SeparatorLine()
            ReadinessRow(name: String("Native dialogs"), text: String("Not checked yet."), state: .unknown)
            SeparatorLine()
            ReadinessRow(name: String("Input service"), text: String("It isn't answering."), state: .notReady)
          }
        }
      }
      .frame(width: SampleWidth.column)
      .sampleSurface()
    }
  }
#endif
