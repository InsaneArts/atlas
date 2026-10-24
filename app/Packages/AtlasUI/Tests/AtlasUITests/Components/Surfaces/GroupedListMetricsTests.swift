import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("GroupedListMetrics")
struct GroupedListMetricsTests {
  @Test("a scrolling grouped list shows three and a half rows before it scrolls")
  func scrollsPastThreeAndAHalfRows() {
    #expect(GroupedListMetrics.scrollHeight(rowHeight: ListRow.Size.regular.height) == 168)
    #expect(!GroupedListMetrics.scrolls(rowCount: 3, rowHeight: ListRow.Size.regular.height))
    #expect(GroupedListMetrics.scrolls(rowCount: 4, rowHeight: ListRow.Size.regular.height))
  }
}
