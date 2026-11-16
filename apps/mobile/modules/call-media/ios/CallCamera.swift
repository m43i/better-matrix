import AVFoundation
import ExpoModulesCore

/// The phone's camera for calls: AVFoundation's frames (NV12) go to the engine over its C entry
/// point, upright by the rotation the connection reports.
final class CallCamera: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
  private let session = AVCaptureSession()
  private let queue = DispatchQueue(label: "call-camera")
  private var observer: NSObjectProtocol?

  private func devices() -> [(String, AVCaptureDevice)] {
    let found = AVCaptureDevice.DiscoverySession(
      deviceTypes: [.builtInWideAngleCamera], mediaType: .video, position: .unspecified
    ).devices
    let front = found.filter { $0.position == .front }.map { ("front", $0) }
    let back = found.filter { $0.position != .front }.enumerated().map { ($0.offset == 0 ? "back" : "back \($0.offset + 1)", $0.element) }
    return front.prefix(1) + back
  }

  func names() -> [String] {
    devices().map { $0.0 }
  }

  func start(_ name: String?) throws {
    stop()
    let all = devices()
    guard let device = all.first(where: { $0.0 == name })?.1 ?? all.first?.1 else {
      throw Exception(name: "NoCamera", description: "There is no camera.")
    }
    let input = try AVCaptureDeviceInput(device: device)
    let output = AVCaptureVideoDataOutput()
    output.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_420YpCbCr8BiPlanarFullRange]
    output.alwaysDiscardsLateVideoFrames = true
    output.setSampleBufferDelegate(self, queue: queue)
    session.beginConfiguration()
    session.sessionPreset = .hd1280x720
    guard session.canAddInput(input), session.canAddOutput(output) else {
      session.commitConfiguration()
      throw Exception(name: "CameraBusy", description: "The camera didn't open.")
    }
    session.addInput(input)
    session.addOutput(output)
    if let connection = output.connection(with: .video), connection.isVideoRotationAngleSupported(90) {
      // Portrait frames: upright for a phone held normally.
      connection.videoRotationAngle = 90
    }
    session.commitConfiguration()
    observer = NotificationCenter.default.addObserver(
      forName: AVCaptureSession.runtimeErrorNotification, object: session, queue: nil
    ) { _ in bm_camera_lost() }
    queue.async { self.session.startRunning() }
  }

  func stop() {
    if let observer { NotificationCenter.default.removeObserver(observer) }
    observer = nil
    queue.sync {
      session.stopRunning()
      session.beginConfiguration()
      session.inputs.forEach { session.removeInput($0) }
      session.outputs.forEach { session.removeOutput($0) }
      session.commitConfiguration()
    }
  }

  func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
    guard let pixels = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
    CVPixelBufferLockBaseAddress(pixels, .readOnly)
    defer { CVPixelBufferUnlockBaseAddress(pixels, .readOnly) }
    guard let y = CVPixelBufferGetBaseAddressOfPlane(pixels, 0), let uv = CVPixelBufferGetBaseAddressOfPlane(pixels, 1) else { return }
    _ = bm_camera_frame_nv12(
      UInt32(CVPixelBufferGetWidth(pixels)), UInt32(CVPixelBufferGetHeight(pixels)), 0,
      y.assumingMemoryBound(to: UInt8.self), UInt32(CVPixelBufferGetBytesPerRowOfPlane(pixels, 0)),
      uv.assumingMemoryBound(to: UInt8.self), UInt32(CVPixelBufferGetBytesPerRowOfPlane(pixels, 1)))
  }
}
