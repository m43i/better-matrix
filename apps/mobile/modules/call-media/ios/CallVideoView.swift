import ExpoModulesCore
import UIKit

/// One video of the call: the engine draws each frame (RGBA, already shaped to the view) through
/// a C callback from its media threads; the view shows it as its layer's contents.
class CallVideoView: ExpoView {
  private var token: UInt64 = 0
  private var video: UInt64 = 0
  private var contain = false

  required init(appContext: AppContext? = nil) {
    super.init(appContext: appContext)
    clipsToBounds = true
    layer.contentsGravity = .resize
    backgroundColor = .black
  }

  func setVideo(_ next: UInt64) {
    guard next != video else { return }
    video = next
    attach()
  }

  func setContain(_ next: Bool) {
    guard next != contain else { return }
    contain = next
    attach()
  }

  private func attach() {
    detach()
    guard video > 0, window != nil else { return }
    let context = Unmanaged.passRetained(self).toOpaque()
    token = bm_video_attach(video, contain, context, { context, width, height, rgba, length in
      guard let context, let rgba else { return }
      let view = Unmanaged<CallVideoView>.fromOpaque(context).takeUnretainedValue()
      view.show(width: Int(width), height: Int(height), rgba: Data(bytes: rgba, count: length))
    })
    resized()
  }

  private func detach() {
    guard token != 0 else { return }
    bm_video_detach(token)
    token = 0
    // Balances the retain in attach: the engine no longer draws into this view.
    Unmanaged.passUnretained(self).release()
  }

  private func resized() {
    guard token != 0 else { return }
    let scale = window?.screen.scale ?? 2
    bm_video_resize(token, UInt32(bounds.width * scale), UInt32(bounds.height * scale))
  }

  override func layoutSubviews() {
    super.layoutSubviews()
    resized()
  }

  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window == nil { detach() } else if token == 0 { attach() }
  }

  private func show(width: Int, height: Int, rgba: Data) {
    guard let provider = CGDataProvider(data: rgba as CFData),
      let image = CGImage(
        width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
        provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    else { return }
    DispatchQueue.main.async { [weak self] in
      self?.layer.contents = image
    }
  }
}
