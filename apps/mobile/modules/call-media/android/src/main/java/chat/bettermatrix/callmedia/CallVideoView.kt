package chat.bettermatrix.callmedia

import android.content.Context
import android.graphics.SurfaceTexture
import android.view.Surface
import android.view.TextureView
import expo.modules.kotlin.AppContext
import expo.modules.kotlin.views.ExpoView

/**
 * One video of the call: a TextureView whose Surface the engine draws each frame into from its
 * media threads (CallMediaNative.attach), shaped to the view in Rust.
 */
class CallVideoView(context: Context, appContext: AppContext) :
  ExpoView(context, appContext), TextureView.SurfaceTextureListener {
  override val shouldUseAndroidLayout = true

  private val texture = TextureView(context)
  private var surface: Surface? = null
  private var token = 0L
  private var video = 0L
  private var contain = false

  init {
    texture.surfaceTextureListener = this
    addView(texture, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
  }

  fun setVideo(next: Long) {
    if (next == video) return
    video = next
    attach()
  }

  fun setContain(next: Boolean) {
    if (next == contain) return
    contain = next
    attach()
  }

  fun setMirror(mirror: Boolean) {
    texture.scaleX = if (mirror) -1f else 1f
  }

  private fun attach() {
    detach()
    val surface = surface ?: return
    if (video <= 0) return
    token = CallMediaNative.attach(surface, video, contain)
    if (token != 0L) CallMediaNative.resize(token, texture.width, texture.height)
  }

  private fun detach() {
    if (token != 0L) {
      CallMediaNative.detach(token)
      token = 0L
    }
  }

  override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) {
    surface = Surface(texture)
    attach()
  }

  override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) {
    if (token != 0L) CallMediaNative.resize(token, width, height)
  }

  override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean {
    detach()
    surface?.release()
    surface = null
    return true
  }

  override fun onSurfaceTextureUpdated(texture: SurfaceTexture) {}

  override fun onAttachedToWindow() {
    super.onAttachedToWindow()
    if (token == 0L) attach()
  }

  override fun onDetachedFromWindow() {
    detach()
    super.onDetachedFromWindow()
  }
}
