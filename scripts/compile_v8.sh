#!/bin/bash -ex

cd libfly/third_party
export PATH="$(pwd)/depot_tools:$PATH"
cd v8

./build/install-build-deps.sh
gclient sync

gn gen out.gn/x64.release --args='
    is_debug = false
    target_cpu = "x64"
    cc_wrapper = "ccache"
    is_official_build = true
    v8_deprecation_warnings = false
    v8_enable_gdbjit = false
    v8_enable_i18n_support = false
    v8_experimental_extra_library_files = []
    v8_extra_library_files = []
    v8_imminent_deprecation_warnings = false
    v8_monolithic = true
    v8_untrusted_code_mitigations = false
    v8_use_external_startup_data = false
    v8_use_snapshot = true'
ninja -C out.gn/x64.release v8_monolith