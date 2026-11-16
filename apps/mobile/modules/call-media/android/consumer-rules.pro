# libwebrtc finds its Java classes by name from native code; the bridge's JNI entry points are
# found by their class's name too.
-keep class livekit.org.webrtc.** { *; }
-keep class chat.bettermatrix.callmedia.CallMediaNative { *; }
