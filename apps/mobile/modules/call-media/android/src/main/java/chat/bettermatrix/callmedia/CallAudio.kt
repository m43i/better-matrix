package chat.bettermatrix.callmedia

import android.content.Context
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build

/**
 * The phone's audio during a call: communication mode (the platform's echo cancellation and the
 * call volume), and whether the call plays on the earpiece or the loudspeaker. libwebrtc's audio
 * device module plays and records; this only routes.
 */
class CallAudio(context: Context) {
  private val manager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
  private var active = false

  fun setActive(on: Boolean) {
    if (on == active) return
    active = on
    if (on) {
      manager.mode = AudioManager.MODE_IN_COMMUNICATION
    } else {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        manager.clearCommunicationDevice()
      } else {
        @Suppress("DEPRECATION")
        manager.isSpeakerphoneOn = false
      }
      manager.mode = AudioManager.MODE_NORMAL
    }
  }

  fun setSpeaker(on: Boolean) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
      val wanted = if (on) AudioDeviceInfo.TYPE_BUILTIN_SPEAKER else AudioDeviceInfo.TYPE_BUILTIN_EARPIECE
      val device = manager.availableCommunicationDevices.firstOrNull { it.type == wanted }
      if (device != null) manager.setCommunicationDevice(device) else manager.clearCommunicationDevice()
    } else {
      @Suppress("DEPRECATION")
      manager.isSpeakerphoneOn = on
    }
  }
}
