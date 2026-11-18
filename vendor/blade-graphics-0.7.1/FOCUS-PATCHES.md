# Focus patches to blade-graphics 0.7.1

Vendored on 2026-11-13 from the crates.io package (MIT); the `etc/` folder of the package (documentation images) is left out.

- `src/vulkan/mod.rs`, `src/gles/mod.rs`: `Frame::is_valid`. `acquire_frame` turns `VK_ERROR_OUT_OF_DATE_KHR` into a frame that `present` silently skips, and nothing told the caller. Under Hyprland with direct scanout and the NVIDIA driver, a window put in fullscreen receives new dmabuf feedback right after its swapchain is created, and every later acquire is out of date: 583 frames drawn, none presented, in the user's log of 2026-11-13. The renderer now sees it and recreates the swapchain.
