import AVFoundation
import ExpoModulesCore

/// Calls' native side on iOS: see modules/call-media/index.ts. libwebrtc sets up the audio
/// session itself; this routes it and runs the camera.
public class CallMediaModule: Module {
  private let camera = CallCamera()

  public func definition() -> ModuleDefinition {
    Name("CallMedia")

    Function("prepare") { () -> Bool in
      true
    }

    Function("cameras") { () -> [String] in
      self.camera.names()
    }

    Function("startCamera") { (device: String?) in
      try self.camera.start(device)
    }

    Function("stopCamera") {
      self.camera.stop()
    }

    Function("setAudioActive") { (active: Bool) in
      let session = AVAudioSession.sharedInstance()
      if active {
        try? session.setCategory(.playAndRecord, mode: .voiceChat, options: [.allowBluetooth, .allowBluetoothA2DP])
        try? session.setActive(true)
      } else {
        try? session.setActive(false, options: .notifyOthersOnDeactivation)
      }
    }

    Function("setSpeaker") { (on: Bool) in
      try? AVAudioSession.sharedInstance().overrideOutputAudioPort(on ? .speaker : .none)
    }

    View(CallVideoView.self) {
      Prop("video") { (view: CallVideoView, video: Int) in
        view.setVideo(UInt64(max(video, 0)))
      }
      Prop("contain") { (view: CallVideoView, contain: Bool?) in
        view.setContain(contain ?? false)
      }
      Prop("mirror") { (view: CallVideoView, mirror: Bool?) in
        view.transform = (mirror ?? false) ? CGAffineTransform(scaleX: -1, y: 1) : .identity
      }
    }
  }
}
