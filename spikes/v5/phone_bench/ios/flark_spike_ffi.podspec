Pod::Spec.new do |s|
  s.name             = 'flark_spike_ffi'
  s.version          = '0.0.1'
  s.summary          = 'Flark v5 parse spike (iOS static library).'
  s.description      = 'Vendors the flark_parse_spike Rust static library for the phone benchmark.'
  s.homepage         = 'https://github.com/danReynolds/flark'
  s.license          = { :file => '../LICENSE' }
  s.author           = { 'Dan Reynolds' => 'me@danreynolds.ca' }
  s.source           = { :path => '.' }
  s.source_files     = 'Classes/**/*'
  s.vendored_libraries = 'libflark_parse_spike.a'
  s.dependency 'Flutter'
  s.platform = :ios, '13.0'
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386',
    'OTHER_LDFLAGS' => '-force_load "${PODS_TARGET_SRCROOT}/libflark_parse_spike.a"',
  }
  s.swift_version = '5.0'
end
