project('sentinel', ['c', 'cpp'],
  version: '0.1.0',
  license: 'GPL-3.0-or-later',
  default_options: [
    'c_std=c23',
    'cpp_std=c++23',
    'warning_level=2',
    'werror=false',
    'buildtype=release',
    'b_pie=true',
    'b_lto=true',
  ],
)

cc = meson.get_compiler('c')
cpp = meson.get_compiler('cpp')

# Security hardening flags
# Note: _FORTIFY_SOURCE is typically set by distro build systems (e.g., Arch makepkg)
hardening_flags = [
  '-fstack-protector-strong',
]

add_project_arguments(cc.get_supported_arguments(hardening_flags), language: 'c')
add_project_arguments(cpp.get_supported_arguments(hardening_flags), language: 'cpp')
add_project_link_arguments(cc.get_supported_link_arguments(['-Wl,-z,relro', '-Wl,-z,now']), language: 'c')
add_project_link_arguments(cpp.get_supported_link_arguments(['-Wl,-z,relro', '-Wl,-z,now']), language: 'cpp')

# Dependencies
pam_dep = cc.find_library('pam')
gtk4_dep = dependency('gtk4', version: '>= 4.10')
adw_dep = dependency('libadwaita-1', version: '>= 1.4')
gtk4_layer_shell_dep = dependency('gtk4-layer-shell-0', version: '>= 1.1.0')

# Configuration
conf_data = configuration_data()
conf_data.set_quoted('VERSION', meson.project_version())
conf_data.set_quoted('HELPER_PATH', get_option('prefix') / get_option('libexecdir') / 'sentinel-helper')
conf_data.set_quoted('CONFIG_PATH', get_option('sysconfdir') / 'security' / 'sentinel.conf')

config_h = configure_file(
  output: 'config.h',
  configuration: conf_data,
)

config_inc = include_directories('.')

# Subdirectories
subdir('pam')
subdir('helper')

# Install configuration
install_data('config/sentinel.conf',
  install_dir: get_option('sysconfdir') / 'security',
)

# Install PAM configuration for polkit
install_data('config/polkit-1',
  install_dir: get_option('sysconfdir') / 'pam.d',
)
