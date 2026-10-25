import SwiftUI

extension EnvironmentValues {
  /// The helper account's picture, `HelperPicture.png`, which the window shell supplies as it does the app's icon;
  /// nil, as in previews, draws the avatar disc alone.
  @Entry public var accountPicture: Image?
}
