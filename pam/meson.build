pam_sentinel = shared_library('pam_sentinel',
  'pam_sentinel.c',
  dependencies: [pam_dep],
  include_directories: config_inc,
  name_prefix: '',  # Output as pam_sentinel.so, not libpam_sentinel.so
  install: true,
  install_dir: get_option('libdir') / 'security',
)
