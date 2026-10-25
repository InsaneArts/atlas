import SwiftUI

/// A badge with a value written into each call.
struct Badge: View {
  var body: some View {
    Text(verbatim: "3")
      .font(.system(size: 11))
      .padding(4)
      .frame(width: 22)
      .foregroundStyle(.secondary)
      .opacity(0.6)
      .offset(x: Spacing.m * 2)
    Image(systemName: "star")
      .font(Typography.font(size: 18, weight: .bold))
  }
}
