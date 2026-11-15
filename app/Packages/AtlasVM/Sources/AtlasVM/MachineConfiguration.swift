import AtlasVMKit
import Foundation
import Virtualization

/// The machine a VM's folder describes, as Apple's framework runs it.
enum MachineConfiguration {
  /// Its MAC address's file.
  static func macAddress(in folder: VMFolder) -> URL { folder.root.appending(path: "MACAddress") }

  /// The machine `folder` holds, made with `settings`: its disk, identity,
  /// one display, NAT, one vsock device and its share, read-only.
  static func make(_ folder: VMFolder, settings: VMSettings) throws -> VZVirtualMachineConfiguration {
    guard
      let model = try PrivateFile.read(folder.hardwareModel, limit: 1 << 16)
        .flatMap(VZMacHardwareModel.init(dataRepresentation:)),
      let identifier = try PrivateFile.read(folder.machineIdentifier, limit: 1 << 16)
        .flatMap(VZMacMachineIdentifier.init(dataRepresentation:)),
      let address = try PrivateFile.read(macAddress(in: folder), limit: 64)
        .flatMap({ VZMACAddress(string: String(decoding: $0, as: UTF8.self)) })
    else {
      throw AtlasVMError.notMade
    }
    let platform = VZMacPlatformConfiguration()
    platform.hardwareModel = model
    platform.machineIdentifier = identifier
    platform.auxiliaryStorage = VZMacAuxiliaryStorage(url: folder.auxiliaryStorage)

    let configuration = VZVirtualMachineConfiguration()
    configuration.platform = platform
    configuration.bootLoader = VZMacOSBootLoader()
    configuration.cpuCount = settings.processors
    configuration.memorySize = settings.memoryBytes

    let graphics = VZMacGraphicsDeviceConfiguration()
    graphics.displays = [
      VZMacGraphicsDisplayConfiguration(
        widthInPixels: settings.displayWidth,
        heightInPixels: settings.displayHeight,
        pixelsPerInch: 80
      )
    ]
    configuration.graphicsDevices = [graphics]

    let disk = try VZDiskImageStorageDeviceAttachment(url: folder.disk, readOnly: false)
    configuration.storageDevices = [VZVirtioBlockDeviceConfiguration(attachment: disk)]

    let network = VZVirtioNetworkDeviceConfiguration()
    network.attachment = VZNATNetworkDeviceAttachment()
    network.macAddress = address
    configuration.networkDevices = [network]

    configuration.socketDevices = [VZVirtioSocketDeviceConfiguration()]
    configuration.keyboards = [VZMacKeyboardConfiguration()]
    configuration.pointingDevices = [VZMacTrackpadConfiguration()]
    configuration.entropyDevices = [VZVirtioEntropyDeviceConfiguration()]

    let tag = VZVirtioFileSystemDeviceConfiguration.macOSGuestAutomountTag
    let share = VZVirtioFileSystemDeviceConfiguration(tag: tag)
    share.share = VZSingleDirectoryShare(directory: VZSharedDirectory(url: folder.share, readOnly: true))
    configuration.directorySharingDevices = [share]

    try configuration.validate()
    return configuration
  }
}
