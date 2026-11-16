package chat.bettermatrix.callmedia

import android.content.Context
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

class CameraException(message: String) : CodedException(message)

/** Calls' native side on Android: see modules/call-media/index.ts. */
class CallMediaModule : Module() {
  private val context: Context
    get() = appContext.reactContext?.applicationContext ?: throw CameraException("no context")

  private val camera by lazy { CallCamera(context) }
  private val audio by lazy { CallAudio(context) }

  override fun definition() = ModuleDefinition {
    Name("CallMedia")

    OnCreate {
      appContext.reactContext?.applicationContext?.let { CallMediaNative.initialize(it) }
    }

    OnDestroy {
      camera.stop()
    }

    Function("prepare") {
      CallMediaNative.initialize(context)
    }

    Function("cameras") {
      camera.names()
    }

    Function("startCamera") { device: String? ->
      camera.start(device)
    }

    Function("stopCamera") {
      camera.stop()
    }

    Function("setAudioActive") { active: Boolean ->
      audio.setActive(active)
    }

    Function("setSpeaker") { on: Boolean ->
      audio.setSpeaker(on)
    }

    View(CallVideoView::class) {
      Prop("video") { view: CallVideoView, video: Long ->
        view.setVideo(video)
      }
      Prop("contain") { view: CallVideoView, contain: Boolean? ->
        view.setContain(contain ?: false)
      }
      Prop("mirror") { view: CallVideoView, mirror: Boolean? ->
        view.setMirror(mirror ?: false)
      }
    }
  }
}
