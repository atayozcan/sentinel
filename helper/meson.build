helper_sources = [
  'main.cpp',
  'window.cpp',
  'session_lock.cpp',
]

helper_inc = include_directories('.')

executable('sentinel-helper',
  helper_sources,
  dependencies: [gtk4_dep, adw_dep, gtk4_layer_shell_dep],
  include_directories: [config_inc, helper_inc],
  install: true,
  install_dir: get_option('libexecdir'),
)
