#if DEBUG
  import SwiftUI

  /// The rows' samples, one component at a time, at a window's width.
  struct RowSamples: View {
    /// Which component the samples show.
    enum Kind: Sendable {
      /// Waiting, current and done steps.
      case step
      /// A plain and a secure field.
      case field
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
      VStack(spacing: Spacing.s) {
        switch kind {
        case .step:
          StepList(
            steps: [
              StepList.Step(
                title: String("Find Atlas in the list"),
                detail: String("Privacy & Security"),
                state: .done
              ),
              StepList.Step(title: String("Turn its switch on"), detail: String("Confirm it"), state: .current),
              StepList.Step(title: String("Come back here"), detail: String("Atlas notices"), state: .waiting),
            ]
          )
        case .field:
          GroupedList(SampleListItem.fields) { field in
            FieldRow(
              label: field.title,
              text: .constant(field.detail ?? String()),
              secure: field.id > 0,
              placeholder: String("Required")
            )
          }
        }
      }
      .frame(width: SampleWidth.column)
      .sampleSurface()
    }
  }
#endif
