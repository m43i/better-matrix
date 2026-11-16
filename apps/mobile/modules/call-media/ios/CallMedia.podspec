Pod::Spec.new do |s|
  s.name           = 'CallMedia'
  s.version        = '0.0.2'
  s.summary        = "Calls' native side: the camera, the audio route and the video view."
  s.description    = s.summary
  s.license        = 'MIT'
  s.author         = 'Better Matrix contributors'
  s.homepage       = 'https://github.com/m43i/better-matrix'
  s.platforms      = { :ios => '17.0' }
  s.swift_version  = '5.9'
  s.source         = { git: 'https://github.com/m43i/better-matrix.git' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'

  s.source_files = '**/*.{h,m,swift}'
  s.public_header_files = 'MatrixCallMedia.h'
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'SWIFT_COMPILATION_MODE' => 'wholemodule'
  }
end
