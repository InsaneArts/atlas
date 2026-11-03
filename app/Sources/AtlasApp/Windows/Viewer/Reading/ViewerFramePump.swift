import AtlasService
import AtlasUI
import CoreGraphics
import Foundation

/// The window's own observer of the frame service, while the window watches: one picture per credit, each shown, then
/// committed, so input may name it. Without a frame service, or once it fails, the reading loop reads pictures over
/// the viewer's address as before, and the pump tries again after a pause.
@MainActor
final class ViewerFramePump {
  /// The detail pictures are read at, on their longest side, in pixels.
  static let longest = 1_600
  /// The pause before trying again after the frame service failed or was not offered.
  static let retryEvery: Duration = .seconds(2)

  /// The viewer service, which says where the frame service is.
  private let service: any ViewerService
  /// Opens the lanes.
  private let connector: any FramePipeConnector
  /// The clock its pauses run on.
  private let clock: any Clock<Duration>
  /// The pump, while the window watches.
  private var pumping: Task<Void, Never>?
  /// Numbers each start, so a pump that was stopped never changes what a later one set.
  private var generation = 0
  /// Whether pictures arrive from the frame service now.
  private(set) var streaming = false

  /// Creates a pump.
  ///
  /// - Parameters:
  ///   - service: The viewer service.
  ///   - connector: Opens the lanes.
  ///   - clock: The clock its pauses run on.
  init(service: any ViewerService, connector: any FramePipeConnector, clock: any Clock<Duration>) {
    self.service = service
    self.connector = connector
    self.clock = clock
  }

  /// Pump pictures into `show` while running; `paused` and `pause` say whether to wait, and how long between
  /// pictures.
  ///
  /// - Parameters:
  ///   - paused: Whether the picture is paused now.
  ///   - pause: How long to wait after each picture.
  ///   - show: Shows a picture and the frame it is named by.
  func start(
    paused: @escaping @MainActor () -> Bool,
    pause: @escaping @MainActor () -> Duration,
    show: @escaping @MainActor (Frame, DesktopPicture.Decoded) -> Void
  ) {
    stop()
    let number = generation
    let current = { [weak self] in self?.generation == number }
    let streaming = { [weak self] (on: Bool) in
      if self?.generation == number { self?.streaming = on }
    }
    pumping = Task { [service, connector, clock] in
      while !Task.isCancelled {
        var client: FrameStreamClient?
        do {
          guard let descriptor = try await service.framePipe() else {
            try await clock.sleep(for: Self.retryEvery)
            continue
          }
          let lanes = try await connector.connect(descriptor)
          let opened = FrameStreamClient(descriptor: descriptor, control: lanes.control, frame: lanes.frame)
          client = opened
          try await opened.open(longest: Self.longest)
          while !Task.isCancelled {
            if paused() {
              try await clock.sleep(for: .milliseconds(250))
              continue
            }
            let picture = try await opened.next(longest: Self.longest)
            guard let decoded = await Self.decode(picture) else { throw ServiceError.unknownScreenSize }
            // Committed before it is shown: input aimed at a picture on screen always finds it. One set aside as
            // too old is skipped for the next.
            guard try await opened.commit(picture) else { continue }
            guard !Task.isCancelled, current() else { break }
            streaming(true)
            show(
              Frame(png: Data(), id: picture.frameKey, revision: picture.revision, display: picture.display),
              decoded
            )
            let rest = pause()
            if rest > .zero { try await clock.sleep(for: rest) }
          }
        } catch {
          streaming(false)
        }
        await client?.close()
        streaming(false)
        try? await clock.sleep(for: Self.retryEvery)
      }
    }
  }

  /// Stops pumping; the window reads pictures over the viewer's address again.
  func stop() {
    generation += 1
    pumping?.cancel()
    pumping = nil
    streaming = false
  }

  /// The picture `picture`'s pixels make, off the main actor; nil when they are not one.
  ///
  /// - Parameter picture: The picture.
  /// - Returns: The picture and its size in points: a point a pixel, as with the viewer's PNGs.
  @concurrent nonisolated static func decode(_ picture: FramePipePicture) async -> DesktopPicture.Decoded? {
    guard
      let provider = CGDataProvider(data: picture.rgb as CFData),
      let space = CGColorSpace(name: CGColorSpace.sRGB),
      let image = CGImage(
        width: picture.width,
        height: picture.height,
        bitsPerComponent: 8,
        bitsPerPixel: 24,
        bytesPerRow: picture.width * 3,
        space: space,
        bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.none.rawValue),
        provider: provider,
        decode: nil,
        shouldInterpolate: true,
        intent: .defaultIntent
      )
    else { return nil }
    let size = CGSize(width: picture.width, height: picture.height)
    return DesktopPicture.Decoded(picture: DesktopPicture(id: picture.frameKey, image: image), size: size)
  }
}
