package chat.bettermatrix.callmedia

import android.content.Context
import livekit.org.webrtc.Camera2Enumerator
import livekit.org.webrtc.CameraVideoCapturer
import livekit.org.webrtc.CapturerObserver
import livekit.org.webrtc.EglBase
import livekit.org.webrtc.SurfaceTextureHelper
import livekit.org.webrtc.VideoFrame

/**
 * The phone's camera for calls, through libwebrtc's own Camera2 capturer. The engine starts and
 * stops it (its NativeCamera, registered from JavaScript); each frame goes to the engine as
 * I420 over JNI, with the rotation that makes it upright.
 */
class CallCamera(private val context: Context) {
  private val enumerator = Camera2Enumerator(context)
  private var capturer: CameraVideoCapturer? = null
  private var helper: SurfaceTextureHelper? = null
  private var egl: EglBase? = null

  /** Camera IDs by name: "front", "back", then "front 2", "back 2", … */
  private fun named(): List<Pair<String, String>> {
    val counts = mutableMapOf<String, Int>()
    return enumerator.deviceNames
      .sortedBy { if (enumerator.isFrontFacing(it)) 0 else 1 }
      .map { id ->
        val kind = if (enumerator.isFrontFacing(id)) "front" else "back"
        val n = (counts[kind] ?: 0) + 1
        counts[kind] = n
        (if (n == 1) kind else "$kind $n") to id
      }
  }

  fun names(): List<String> = named().map { it.first }

  @Synchronized
  fun start(device: String?) {
    stop()
    val cameras = named()
    val id = cameras.firstOrNull { it.first == device }?.second
      ?: cameras.firstOrNull()?.second
      ?: throw CameraException("There is no camera.")
    val egl = EglBase.create()
    val helper = SurfaceTextureHelper.create("call-camera", egl.eglBaseContext)
    val capturer = enumerator.createCapturer(id, events)
      ?: run {
        helper.dispose()
        egl.release()
        throw CameraException("The camera didn't open.")
      }
    capturer.initialize(helper, context, observer)
    capturer.startCapture(WIDTH, HEIGHT, FPS)
    this.egl = egl
    this.helper = helper
    this.capturer = capturer
  }

  @Synchronized
  fun stop() {
    capturer?.let {
      try {
        it.stopCapture()
      } catch (_: InterruptedException) {
      }
      it.dispose()
    }
    helper?.dispose()
    egl?.release()
    capturer = null
    helper = null
    egl = null
  }

  private val observer = object : CapturerObserver {
    override fun onCapturerStarted(success: Boolean) {
      if (!success) CallMediaNative.cameraLost()
    }

    override fun onCapturerStopped() {}

    override fun onFrameCaptured(frame: VideoFrame) {
      val i420 = frame.buffer.toI420() ?: return
      try {
        CallMediaNative.cameraFrame(
          i420.width,
          i420.height,
          frame.rotation,
          i420.dataY,
          i420.strideY,
          i420.dataU,
          i420.strideU,
          i420.dataV,
          i420.strideV,
        )
      } finally {
        i420.release()
      }
    }
  }

  private val events = object : CameraVideoCapturer.CameraEventsHandler {
    override fun onCameraError(error: String?) = CallMediaNative.cameraLost()
    override fun onCameraDisconnected() = CallMediaNative.cameraLost()
    override fun onCameraFreezed(error: String?) {}
    override fun onCameraOpening(name: String?) {}
    override fun onFirstFrameAvailable() {}
    override fun onCameraClosed() {}
  }

  private companion object {
    const val WIDTH = 1280
    const val HEIGHT = 720
    const val FPS = 30
  }
}
