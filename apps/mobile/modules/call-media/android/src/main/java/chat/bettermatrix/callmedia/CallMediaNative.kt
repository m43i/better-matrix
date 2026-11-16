package chat.bettermatrix.callmedia

import android.content.Context
import android.view.Surface
import java.nio.ByteBuffer

/**
 * The engine's JNI entry points (packages/matrix-ffi/src/android.rs). Loading the library here
 * runs its JNI_OnLoad, which hands libwebrtc the Java VM; it is the same libmatrix_ffi.so the
 * turbo module links against, so the engine and these calls share one process-wide state.
 */
object CallMediaNative {
  init {
    System.loadLibrary("matrix_ffi")
  }

  /** Gives libwebrtc the application context, which its audio device module needs. */
  @JvmStatic external fun initialize(context: Context): Boolean

  /** Draws video [video] into [surface] until [detach]; returns the view's token, 0 on failure. */
  @JvmStatic external fun attach(surface: Surface, video: Long, contain: Boolean): Long

  @JvmStatic external fun resize(token: Long, width: Int, height: Int)

  @JvmStatic external fun detach(token: Long)

  /** One camera frame (direct buffers), [rotation] degrees clockwise to stand upright. */
  @JvmStatic external fun cameraFrame(
    width: Int,
    height: Int,
    rotation: Int,
    y: ByteBuffer,
    strideY: Int,
    u: ByteBuffer,
    strideU: Int,
    v: ByteBuffer,
    strideV: Int,
  ): Boolean

  /** The camera stopped on its own. */
  @JvmStatic external fun cameraLost()
}
