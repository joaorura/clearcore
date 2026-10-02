project(
    'pipewire_helper',
    'c',
    version: '0.1.0',
    default_options: [
        'c_std=gnu11',
        'warning_level=2',
    ]
)

cc = meson.get_compiler('c')

pipewire_dep = dependency('libpipewire-0.3', required: true)
threads_dep = dependency('threads')
m_dep = cc.find_library('m', required: false)
dl_dep = cc.find_library('dl', required: false)

inc_dir = include_directories('src')

helper_sources = files(
    'src/pipewire_helper.c',
    'src/transport_bridge.c',
    'src/format_converter.c',
    'src/noise_suppressor.c'
)

executable(
    'pipewire_helper',
    helper_sources,
    include_directories: inc_dir,
    dependencies: [pipewire_dep, threads_dep, m_dep, dl_dep],
    install: true
)

test_sources = files(
    'tests/test_callback.c',
    'src/transport_bridge.c',
    'src/format_converter.c',
    'src/noise_suppressor.c'
)

test_callback_contract = executable(
    'test_callback_contract',
    test_sources,
    include_directories: inc_dir,
    dependencies: [pipewire_dep, threads_dep, m_dep],
    link_args: ['-Wl,--wrap=malloc', '-Wl,--wrap=calloc']
)

test('realtime_callback_contract', test_callback_contract)

test_target_resolver = executable(
    'test_target_resolver',
    files('tests/test_target_resolver.c'),
    include_directories: inc_dir
)

test('target_resolver_contract', test_target_resolver)

test_shared_state = executable(
    'test_shared_state',
    'tests/test_shared_state.c',
    include_directories: inc_dir
)

test('shared_state_layout', test_shared_state)
