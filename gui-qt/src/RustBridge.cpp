#include "RustBridge.h"

#include "kokoromi_core.h"

namespace {

thread_local std::vector<std::string> g_collected;

extern "C" void collectLine(const char *line) {
    if (line != nullptr) {
        g_collected.emplace_back(line);
    }
}

}

namespace RustBridge {

std::vector<std::string> runCpuTests() {
    g_collected.clear();
    kokoromi_run_cpu_tests(&collectLine);
    return std::move(g_collected);
}

std::vector<std::string> runGpuTests() {
    g_collected.clear();
    kokoromi_run_gpu_tests(&collectLine);
    return std::move(g_collected);
}

bool renderTestFrame() {
    return kokoromi_render_test_frame();
}

bool renderDemoFrame(float seconds) {
    return kokoromi_render_demo_frame(seconds);
}

bool renderMandelbrotFrame(float seconds) {
    return kokoromi_render_mandelbrot_frame(seconds);
}

bool renderTriangleDemo(float seconds) {
    return kokoromi_render_triangle_demo(seconds);
}

bool flushDemoCaches() {
    return kokoromi_flush_demo_caches();
}

bool renderRaytracedDemo(float seconds) {
    return kokoromi_render_raytraced_demo(seconds);
}

void setRaytracingEnabled(bool enabled) {
    kokoromi_set_raytracing_enabled(enabled);
}

bool renderHelmetDemo(float seconds) {
    return kokoromi_render_helmet_demo(seconds);
}

bool renderPostFrame(float seconds) {
    return kokoromi_render_post_frame(seconds);
}

bool renderSpritesFrame(float seconds) {
    return kokoromi_render_sprites_frame(seconds);
}

bool renderTensorUpscaleFrame(float seconds) {
    return kokoromi_render_tensor_upscale_frame(seconds);
}

bool renderMusicDemo(float seconds) {
    return kokoromi_render_music_demo(seconds);
}

bool renderBounceDemo(float seconds) {
    return kokoromi_render_bounce_demo(seconds);
}

bool renderParticlesFrame(float seconds) {
    return kokoromi_render_particles_frame(seconds);
}

bool renderSolarFrame(float) {
    return kokoromi_render_solar_frame(false);
}

bool renderSolarShowcaseFrame(float) {
    return kokoromi_render_solar_frame(true);
}

}

bool RustBridge::renderDungeonFrame(float seconds) {
    return kokoromi_render_dungeon_frame(seconds);
}
