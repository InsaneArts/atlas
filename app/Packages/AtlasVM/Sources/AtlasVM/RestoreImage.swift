import AtlasVMKit
import Darwin
import Foundation
import Virtualization

/// The macOS a VM is installed from: the newest restore image Apple's
/// framework supports for this Mac, kept once downloaded and checked.
enum RestoreImage {
  /// Where restore images are kept, beside every VM's folder.
  static func folder(in parent: URL) -> URL {
    parent.appending(path: "RestoreImages", directoryHint: .isDirectory)
  }

  /// The newest kept restore image, if any.
  static func kept(in folder: URL) -> URL? {
    let names = (try? FileManager.default.contentsOfDirectory(atPath: folder.path(percentEncoded: false))) ?? []
    return names.filter { $0.hasSuffix(".ipsw") }
      .sorted().last
      .map { folder.appending(path: $0) }
  }

  /// Which restore image to make from, kept in `folder`: the one the
  /// framework names, kept or to download; when the framework cannot be
  /// asked, the newest one kept.
  static func choose(in folder: URL) async throws -> RestoreImageChoice {
    try VMFolder.makePrivate(folder)
    let latest: VZMacOSRestoreImage
    do {
      latest = try await VZMacOSRestoreImage.latestSupported
    } catch {
      guard let kept = kept(in: folder) else { throw AtlasVMError.noRestoreImage }
      return RestoreImageChoice(file: kept, source: nil)
    }
    let file = folder.appending(path: "\(latest.buildVersion).ipsw")
    let present = FileManager.default
      .fileExists(atPath: file.path(percentEncoded: false))
    return RestoreImageChoice(file: file, source: present ? nil : latest.url)
  }

  /// The chosen image, downloaded first when it is not kept, its bytes
  /// told to `log` as they come: one run at a time, so two never write or
  /// check one image together.
  static func fetch(_ choice: RestoreImageChoice, log: MakingLog) async throws -> URL {
    guard let source = choice.source else { return choice.file }
    let cache = choice.file.deletingLastPathComponent()
    let lock = try await FileLock.waited(at: cache.appending(path: ".lock"))
    defer { withExtendedLifetime(lock) {} }
    // Another run may have kept it while this one waited.
    if FileManager.default.fileExists(atPath: choice.file.path(percentEncoded: false)) {
      return choice.file
    }
    try await download(source, to: choice.file, log: log)
    return choice.file
  }

  /// The lock every install of macOS in a VM holds from before it begins
  /// until its scratch is removed, kept beside the images in `folder`: one
  /// install of Atlas VM's at a time, so each tells its own scratch apart.
  static func installing(in folder: URL) async throws -> FileLock {
    try await FileLock.waited(at: folder.appending(path: "Installing.lock"))
  }

  /// Download `url` to `file`, resuming from what a cut download kept, its
  /// bytes told to `log` on either path, and keep it only once it is whole
  /// and loads as a restore image.
  private static func download(_ url: URL, to file: URL, log: MakingLog) async throws {
    let resuming = file.appendingPathExtension("resume")
    let downloaded: URL
    let response: URLResponse
    let watch = DownloadWatch(log: log)
    do {
      if let data = try? Data(contentsOf: resuming) {
        (downloaded, response) = try await URLSession.shared
          .download(resumeFrom: data, delegate: watch)
      } else {
        (downloaded, response) = try await URLSession.shared
          .download(from: url, delegate: watch)
      }
    } catch {
      if let data = (error as NSError).userInfo[NSURLSessionDownloadTaskResumeData] as? Data {
        try? data.write(to: resuming)
      }
      throw error
    }
    try? FileManager.default
      .removeItem(at: resuming)
    // Whole, or the rest of a resumed one.
    guard let status = (response as? HTTPURLResponse)?.statusCode, status == 200 || status == 206 else {
      try? FileManager.default
        .removeItem(at: downloaded)
      throw AtlasVMError.noRestoreImage
    }
    // Checked under a name of this run's, kept only once it loads.
    let checking = file.appendingPathExtension("checking-\(getpid())")
    try? FileManager.default
      .removeItem(at: checking)
    try FileManager.default
      .moveItem(at: downloaded, to: checking)
    do {
      _ = try await VZMacOSRestoreImage.image(from: checking)
    } catch {
      try? FileManager.default
        .removeItem(at: checking)
      throw AtlasVMError.noRestoreImage
    }
    try FileManager.default
      .moveItem(at: checking, to: file)
  }
}
