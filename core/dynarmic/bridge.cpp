// Dynarmic cores share the monitor and RAM but own JIT state. Fastmem is used until GPU mappings
// need write observation; then the core runs through callbacks so dirty pages and exclusives agree.
#include <dynarmic/interface/A64/a64.h>
#include <dynarmic/interface/exclusive_monitor.h>
#include <algorithm>
#include <array>
#include <chrono>
#include <cstdint>
#include <cstring>
#include <limits>
#include <memory>
#include <utility>
#include <vector>

namespace {
constexpr std::uint64_t tick_hz = 19'200'000;
constexpr std::uint64_t memory_base = 0x8000'0000;
constexpr std::size_t page_table_bits = 34;
using WriteCallback = void (*)(void*, std::uint64_t, std::size_t);
using ReadBus = bool (*)(void*, std::uint64_t, std::size_t, std::uint64_t*);
using WriteBus = bool (*)(void*, std::uint64_t, std::size_t, std::uint64_t);
using Sysreg = bool (*)(void*, std::uint32_t, std::uint32_t, std::uint32_t, std::uint32_t, std::uint32_t, bool, std::uint64_t*);

struct Context {
    std::uint64_t x[31], v[64], sp, pc, tls, tpidr, daif;
    std::uint32_t pstate, fpcr, fpsr;
};
struct Monitor { Dynarmic::ExclusiveMonitor value{8}; };
struct TlbEntry {
    std::uint64_t tag{std::numeric_limits<std::uint64_t>::max()}, pa{};
    std::uint8_t permissions{}, fault{};
};

struct Callbacks final : Dynarmic::A64::UserCallbacks {
    std::uint8_t* memory{};
    std::uint64_t size{};
    WriteCallback write_callback{};
    ReadBus read_bus{};
    WriteBus write_bus{};
    Sysreg sysreg{};
    void* write_data{};
    Dynarmic::A64::Jit* jit{};
    std::uint64_t tls{}, tpidr{}, daif{}, ticks{}, deadline{std::numeric_limits<std::uint64_t>::max()}, tick_scale{192};
    std::uint64_t sctlr{}, tcr{}, ttbr0{}, ttbr1{}, mair{}, vbar{}, spsr{}, elr{}, esr{}, far{};
    std::uint64_t sp_el0{}, sp_el1{}, cntv_ctl{}, cntv_cval{}, exception_pc{}, abort_esr{}, abort_far{};
    std::array<TlbEntry, 256> tlb{};
    std::vector<void*> page_table;
    std::uint32_t svc{std::numeric_limits<std::uint32_t>::max()}, core{};
    std::uint8_t current_el{1};
    bool virtual_time{}, fault{}, resume{}, guest_kernel{}, watched{}, irq{}, fiq{}, waiting{}, exception_pending{}, abort_pending{};

    std::uint64_t now() const {
        if (virtual_time) return ticks;
        const auto ns = std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now().time_since_epoch()).count();
        return static_cast<std::uint64_t>(ns) * tick_hz / 1'000'000'000;
    }
    bool physical(std::uint64_t address, std::size_t length, std::uint64_t& offset) const {
        const auto base = guest_kernel ? memory_base : 0;
        if (address < base || address - base > size || length > size - (address - base)) return false;
        offset = address - base;
        return true;
    }
    void stop_fault() { fault = true; if (jit) jit->HaltExecution(Dynarmic::HaltReason::MemoryAbort); }
    void clear_tlb() {
        for (auto& entry : tlb) entry.tag = std::numeric_limits<std::uint64_t>::max();
        std::fill(page_table.begin(), page_table.end(), nullptr);
    }
    void invalidate() {
        clear_tlb();
        if (jit) jit->ClearCache();
    }
    void cache_page(std::uint64_t va, std::uint64_t pa, std::uint8_t permissions) {
        std::uint64_t offset;
        if (!page_table.empty() && !watched && permissions == 7 && va < (std::uint64_t{1} << page_table_bits) && physical(pa & ~0xfff, 0x1000, offset))
            page_table[va >> 12] = memory + offset;
    }
    std::uint8_t walk(std::uint64_t va, std::uint8_t access, std::uint64_t& pa, std::uint8_t& permissions) {
        if (!(sctlr & 1)) { pa = va; permissions = 7; return 0; }
        const bool upper = va >> 63;
        const auto size_bits = upper ? tcr >> 16 & 63 : tcr & 63;
        const auto bits = 64 - size_bits;
        const bool disabled = tcr >> (upper ? 23 : 7) & 1;
        const auto granule = upper ? tcr >> 30 & 3 : tcr >> 14 & 3;
        if (disabled || bits < 25 || bits > 48 || granule != (upper ? 2 : 0) || (upper ? ~va : va) >> bits) return 4;
        const auto tag = va >> 12 | std::uint64_t(current_el == 0) << 52;
        auto& cached = tlb[tag % tlb.size()];
        if (cached.tag == tag) {
            pa = cached.pa | (va & 0xfff); permissions = cached.permissions;
            return permissions & access ? 0 : cached.fault;
        }
        auto level = 3U - static_cast<unsigned>((bits - 1 - 12) / 9);
        auto table = (upper ? ttbr1 : ttbr0) & 0x0000'ffff'ffff'f000;
        bool user_table = true, write_table = true, px_table = true, ux_table = true;
        std::uint64_t descriptor{}, shift{};
        for (;;) {
            shift = 12 + 9 * (3 - level);
            std::uint64_t offset;
            if (!physical(table + ((va >> shift) & 0x1ff) * 8, 8, offset)) return static_cast<std::uint8_t>(20 + level);
            std::memcpy(&descriptor, memory + offset, 8);
            if ((descriptor & 3) == 3 && level < 3) {
                user_table &= !(descriptor >> 61 & 1); write_table &= !(descriptor >> 62 & 1);
                px_table &= !(descriptor >> 59 & 1); ux_table &= !(descriptor >> 60 & 1);
                table = descriptor & 0x0000'ffff'ffff'f000; ++level; continue;
            }
            if (((descriptor & 3) == 3 && level == 3) || ((descriptor & 3) == 1 && level > 0 && level < 3)) break;
            return static_cast<std::uint8_t>(4 + level);
        }
        if (!(descriptor >> 10 & 1)) return static_cast<std::uint8_t>(8 + level);
        const auto ap = descriptor >> 6 & 3;
        const bool user = (ap & 1) && user_table, writable = !(ap & 2) && write_table;
        const bool readable = current_el || user;
        const bool executable = current_el ? !(descriptor >> 53 & 1) && px_table : !(descriptor >> 54 & 1) && ux_table;
        permissions = (readable ? 1 : 0) | (readable && writable ? 2 : 0) | (executable && (!(sctlr >> 19 & 1) || !writable) ? 4 : 0);
        if (!(permissions & access)) return static_cast<std::uint8_t>(12 + level);
        const auto block = (std::uint64_t{1} << shift) - 1;
        pa = (descriptor & 0x0000'ffff'ffff'f000 & ~block) | (va & block);
        cached = {tag, pa & ~0xfff, permissions, static_cast<std::uint8_t>(12 + level)};
        return 0;
    }
    void take_exception(std::uint64_t syndrome, std::uint64_t address, std::uint64_t pc, std::uint64_t offset) {
        const bool lower = current_el == 0;
        spsr = jit->GetPstate() | daif | (lower ? 0 : 5); elr = pc; esr = syndrome; far = address;
        if (lower) { sp_el0 = jit->GetSP(); jit->SetSP(sp_el1); clear_tlb(); }
        current_el = 1; daif |= 0x3c0; waiting = false;
        exception_pc = vbar + (lower ? 0x400 : 0x200) + offset;
        exception_pending = true;
        jit->SetPC(exception_pc);
    }
    void abort(std::uint64_t va, std::uint8_t access, std::uint8_t status) {
        if (!guest_kernel) { stop_fault(); return; }
        const auto ec = access == 4 ? (current_el ? 0x21 : 0x20) : (current_el ? 0x25 : 0x24);
        abort_esr = std::uint64_t(ec) << 26 | std::uint64_t(access == 2) << 6 | status;
        abort_far = va; abort_pending = true;
        jit->HaltExecution(Dynarmic::HaltReason::MemoryAbort);
    }
    template<typename T> T read(std::uint64_t address, std::uint8_t access = 1) {
        T value{}; auto* out = reinterpret_cast<std::uint8_t*>(&value);
        for (std::size_t done = 0; done < sizeof(T);) {
            std::uint64_t pa; std::uint8_t permissions;
            if (const auto status = walk(address + done, access, pa, permissions)) { abort(address + done, access, status); break; }
            const auto chunk = std::min(sizeof(T) - done, static_cast<std::size_t>(0x1000 - (pa & 0xfff)));
            std::uint64_t offset;
            if (physical(pa, chunk, offset)) { std::memcpy(out + done, memory + offset, chunk); cache_page(address + done, pa, permissions); }
            else {
                std::uint64_t value{};
                if (!read_bus || !read_bus(write_data, pa, chunk, &value)) { abort(address + done, access, 0x10); break; }
                std::memcpy(out + done, &value, chunk);
            }
            done += chunk;
        }
        return value;
    }
    template<typename T> void write(std::uint64_t address, T value) {
        const auto* in = reinterpret_cast<const std::uint8_t*>(&value);
        for (std::size_t done = 0; done < sizeof(T);) {
            std::uint64_t pa; std::uint8_t permissions;
            if (const auto status = walk(address + done, 2, pa, permissions)) { abort(address + done, 2, status); return; }
            const auto chunk = std::min(sizeof(T) - done, static_cast<std::size_t>(0x1000 - (pa & 0xfff)));
            std::uint64_t offset;
            if (physical(pa, chunk, offset)) {
                std::memcpy(memory + offset, in + done, chunk);
                if (write_callback) write_callback(write_data, offset, chunk);
                cache_page(address + done, pa, permissions);
            } else {
                std::uint64_t value{}; std::memcpy(&value, in + done, chunk);
                if (!write_bus || !write_bus(write_data, pa, chunk, value)) { abort(address + done, 2, 0x10); return; }
            }
            done += chunk;
        }
    }
    template<typename T> bool exclusive(std::uint64_t address, T value, T expected) {
        const auto current = read<T>(address);
        if (abort_pending || std::memcmp(&current, &expected, sizeof(T))) return false;
        write(address, value); return true;
    }
    std::optional<std::uint32_t> MemoryReadCode(std::uint64_t address) override {
        std::uint64_t pa; std::uint8_t permissions;
        if (const auto status = walk(address, 4, pa, permissions)) { abort(address, 4, status); return std::nullopt; }
        std::uint64_t offset;
        if (!physical(pa, 4, offset)) { abort(address, 4, 0x10); return std::nullopt; }
        std::uint32_t value; std::memcpy(&value, memory + offset, 4); return value;
    }
    std::uint8_t MemoryRead8(std::uint64_t a) override { return read<std::uint8_t>(a); }
    std::uint16_t MemoryRead16(std::uint64_t a) override { return read<std::uint16_t>(a); }
    std::uint32_t MemoryRead32(std::uint64_t a) override { return read<std::uint32_t>(a); }
    std::uint64_t MemoryRead64(std::uint64_t a) override { return read<std::uint64_t>(a); }
    Dynarmic::A64::Vector MemoryRead128(std::uint64_t a) override { const auto lo = read<std::uint64_t>(a); return {lo, abort_pending ? 0 : read<std::uint64_t>(a + 8)}; }
    void MemoryWrite8(std::uint64_t a, std::uint8_t v) override { write(a, v); }
    void MemoryWrite16(std::uint64_t a, std::uint16_t v) override { write(a, v); }
    void MemoryWrite32(std::uint64_t a, std::uint32_t v) override { write(a, v); }
    void MemoryWrite64(std::uint64_t a, std::uint64_t v) override { write(a, v); }
    void MemoryWrite128(std::uint64_t a, Dynarmic::A64::Vector v) override { write(a, v[0]); if (!abort_pending) write(a + 8, v[1]); }
    bool MemoryWriteExclusive8(std::uint64_t a, std::uint8_t v, std::uint8_t e) override { return exclusive(a, v, e); }
    bool MemoryWriteExclusive16(std::uint64_t a, std::uint16_t v, std::uint16_t e) override { return exclusive(a, v, e); }
    bool MemoryWriteExclusive32(std::uint64_t a, std::uint32_t v, std::uint32_t e) override { return exclusive(a, v, e); }
    bool MemoryWriteExclusive64(std::uint64_t a, std::uint64_t v, std::uint64_t e) override { return exclusive(a, v, e); }
    bool MemoryWriteExclusive128(std::uint64_t a, Dynarmic::A64::Vector v, Dynarmic::A64::Vector e) override { return exclusive(a, v, e); }

    void InterpreterFallback(std::uint64_t pc, std::size_t count) override {
        (void)count;
        const auto instruction = read<std::uint32_t>(pc, 4);
        const auto reg = instruction & 31, key = instruction >> 5 & 0xffff;
        const auto value = reg == 31 ? 0 : jit->GetRegister(reg);
        auto set = [&](std::uint64_t result) { if (reg != 31) jit->SetRegister(reg, result); };
        if (instruction == 0xd69f'03e0) {
            if (!current_el) take_exception(0, 0, pc, 0);
            else {
                const bool lower = (spsr & 0xf) == 0;
                current_el = lower ? 0 : 1; jit->SetPstate(spsr); daif = spsr & 0x3c0; jit->SetPC(elr);
                if (lower) { sp_el1 = jit->GetSP(); jit->SetSP(sp_el0); clear_tlb(); }
            }
        } else if ((instruction & 0xffff'f000) == 0xd508'8000) {
            if (!current_el) take_exception(0, 0, pc, 0); else { invalidate(); jit->SetPC(pc + 4); }
        } else if ((instruction & 0xfff0'0000) == 0xd510'0000) {
            const bool timer = key >= 0xdf18 && key <= 0xdf1a;
            if (!current_el && !timer) take_exception(0, 0, pc, 0);
            else {
                switch (key) {
                case 0xc080: sctlr = value; invalidate(); break;
                case 0xc100: ttbr0 = value; invalidate(); break;
                case 0xc101: ttbr1 = value; invalidate(); break;
                case 0xc102: tcr = value; invalidate(); break;
                case 0xc200: spsr = value; break;
                case 0xc201: elr = value; break;
                case 0xc208: sp_el0 = value; break;
                case 0xc510: mair = value; break;
                case 0xc600: vbar = value; break;
                case 0xda11: daif = value & 0x3c0; break;
                case 0xdf18: cntv_cval = now() + static_cast<std::int32_t>(value); break;
                case 0xdf19: cntv_ctl = value & 3; break;
                case 0xdf1a: cntv_cval = value; break;
                default: {
                    const auto op0 = instruction >> 19 & 3, op1 = instruction >> 16 & 7;
                    const auto crn = instruction >> 12 & 15, crm = instruction >> 8 & 15, op2 = instruction >> 5 & 7;
                    auto forwarded = value;
                    if (!sysreg || !sysreg(write_data, op0, op1, crn, crm, op2, true, &forwarded)) take_exception(0, 0, pc, 0);
                    break;
                }
                }
                if (jit->GetPC() == pc) jit->SetPC(pc + 4);
            }
        } else if ((instruction & 0xfff0'0000) == 0xd530'0000) {
            const bool allowed = current_el || key >= 0xdf00;
            std::uint64_t result{};
            if (!allowed) take_exception(0, 0, pc, 0);
            else {
                switch (key) {
                case 0xc080: result = sctlr; break; case 0xc100: result = ttbr0; break;
                case 0xc101: result = ttbr1; break; case 0xc102: result = tcr; break;
                case 0xc200: result = spsr; break; case 0xc201: result = elr; break;
                case 0xc208: result = sp_el0; break; case 0xc210: result = current_el ? 1 : 0; break;
                case 0xc212: result = std::uint64_t(current_el) << 2; break; case 0xc290: result = esr; break;
                case 0xc300: result = far; break; case 0xc510: result = mair; break;
                case 0xc600: result = vbar; break; case 0xc005: result = 0x8000'0000 | core; break;
                case 0xda11: result = daif; break; case 0xdf00: result = tick_hz; break;
                case 0xdf01: case 0xdf02: result = now(); break;
                case 0xdf18: result = static_cast<std::uint32_t>(cntv_cval - now()); break;
                case 0xdf19: result = cntv_ctl | std::uint64_t((cntv_ctl & 1) && now() >= cntv_cval) << 2; break;
                case 0xdf1a: result = cntv_cval; break; default: {
                    const auto op0 = instruction >> 19 & 3, op1 = instruction >> 16 & 7;
                    const auto crn = instruction >> 12 & 15, crm = instruction >> 8 & 15, op2 = instruction >> 5 & 7;
                    if (!sysreg || !sysreg(write_data, op0, op1, crn, crm, op2, false, &result)) take_exception(0, 0, pc, 0);
                    break;
                }
                }
                set(result); if (jit->GetPC() == pc) jit->SetPC(pc + 4);
            }
        } else take_exception(0, 0, pc, 0);
        resume = true; jit->HaltExecution(Dynarmic::HaltReason::UserDefined2);
    }
    void CallSVC(std::uint32_t value) override {
        if (!guest_kernel) { svc = value; jit->HaltExecution(); return; }
        take_exception(std::uint64_t(0x15) << 26 | value, 0, jit->GetPC(), 0);
        resume = true; jit->HaltExecution(Dynarmic::HaltReason::UserDefined2);
    }
    void ExceptionRaised(std::uint64_t pc, Dynarmic::A64::Exception exception) override {
        if (guest_kernel && exception == Dynarmic::A64::Exception::WaitForInterrupt) {
            jit->SetPC(pc + 4); waiting = true; jit->HaltExecution();
        } else if (guest_kernel) {
            take_exception(0, 0, pc, 0); resume = true; jit->HaltExecution(Dynarmic::HaltReason::UserDefined2);
        } else stop_fault();
    }
    void InstructionCacheOperationRaised(Dynarmic::A64::InstructionCacheOperation, std::uint64_t) override { jit->ClearCache(); }
    void AddTicks(std::uint64_t value) override { if (virtual_time) ticks += value * tick_scale; }
    std::uint64_t GetTicksRemaining() override { return deadline - std::min(deadline, now()); }
    std::uint64_t GetCNTPCT() override { return now(); }
    bool interrupt_pending() const { return irq || ((cntv_ctl & 3) == 1 && now() >= cntv_cval); }
    void deliver_interrupts() {
        if (fiq && !(daif & 0x40)) take_exception(0, 0, jit->GetPC(), 0x100);
        else if (interrupt_pending() && !(daif & 0x80)) take_exception(0, 0, jit->GetPC(), 0x80);
    }
};

struct Cpu {
    Cpu(Monitor* monitor_, std::size_t id_, std::uint8_t* memory_, std::uint64_t size_, WriteCallback callback_, ReadBus read_bus_, WriteBus write_bus_, Sysreg sysreg_, void* data_) : monitor(monitor_), id(id_) {
        callbacks.memory = memory_; callbacks.size = size_; callbacks.write_callback = callback_; callbacks.read_bus = read_bus_;
        callbacks.write_bus = write_bus_; callbacks.sysreg = sysreg_; callbacks.write_data = data_; rebuild(true);
    }
    Dynarmic::A64::UserConfig config(bool fastmem) {
        Dynarmic::A64::UserConfig config{&callbacks};
        config.processor_id = id; config.global_monitor = &monitor->value; config.cntfrq_el0 = tick_hz;
        config.tpidrro_el0 = &callbacks.tls; config.tpidr_el0 = &callbacks.tpidr;
        config.fastmem_exclusive_access = false; config.silently_mirror_fastmem = false; config.fastmem_address_space_bits = 34;
        config.check_halt_on_memory_access = callbacks.guest_kernel; config.hook_hint_instructions = callbacks.guest_kernel;
        if (callbacks.guest_kernel && !callbacks.watched) {
            config.page_table = callbacks.page_table.data(); config.page_table_address_space_bits = page_table_bits;
            config.silently_mirror_page_table = false; config.detect_misaligned_access_via_page_table = 8 | 16 | 32 | 64 | 128;
            config.only_detect_misalignment_via_page_table_on_page_boundary = true;
        }
        if (fastmem && !callbacks.guest_kernel) config.fastmem_pointer = reinterpret_cast<std::uintptr_t>(callbacks.memory);
        return config;
    }
    void save(Context& out) const {
        const auto x = jit->GetRegisters(); const auto v = jit->GetVectors(); std::copy(x.begin(), x.end(), out.x);
        for (std::size_t i = 0; i < v.size(); ++i) { out.v[i * 2] = v[i][0]; out.v[i * 2 + 1] = v[i][1]; }
        out.sp = jit->GetSP(); out.pc = jit->GetPC(); out.tls = callbacks.tls; out.tpidr = callbacks.tpidr; out.daif = callbacks.daif;
        out.pstate = jit->GetPstate(); out.fpcr = jit->GetFpcr(); out.fpsr = jit->GetFpsr();
    }
    void load(const Context& in) {
        std::array<std::uint64_t, 31> x; std::copy(std::begin(in.x), std::end(in.x), x.begin()); std::array<Dynarmic::A64::Vector, 32> v;
        for (std::size_t i = 0; i < v.size(); ++i) v[i] = {in.v[i * 2], in.v[i * 2 + 1]};
        jit->SetRegisters(x); jit->SetVectors(v); jit->SetSP(in.sp); jit->SetPC(in.pc); callbacks.tls = in.tls; callbacks.tpidr = in.tpidr; callbacks.daif = in.daif;
        jit->SetPstate(in.pstate); jit->SetFpcr(in.fpcr); jit->SetFpsr(in.fpsr);
    }
    void rebuild(bool fastmem) {
        Context state{}; if (jit) save(state); jit.reset(); jit = std::make_unique<Dynarmic::A64::Jit>(config(fastmem)); callbacks.jit = jit.get();
        if (initialized) load(state); initialized = true; using_fastmem = fastmem && !callbacks.guest_kernel;
    }
    Monitor* monitor; std::size_t id; Callbacks callbacks; std::unique_ptr<Dynarmic::A64::Jit> jit; bool initialized{}, using_fastmem{};
};
}

extern "C" {
Monitor* kokoromi_dynarmic_monitor_new() { return new Monitor; }
void kokoromi_dynarmic_monitor_free(Monitor* monitor) { delete monitor; }
Cpu* kokoromi_dynarmic_cpu_new(Monitor* monitor, std::size_t id, std::uint8_t* memory, std::uint64_t size, WriteCallback callback, ReadBus read_bus, WriteBus write_bus, Sysreg sysreg, void* data) {
    try { return new Cpu(monitor, id, memory, size, callback, read_bus, write_bus, sysreg, data); } catch (...) { return nullptr; }
}
void kokoromi_dynarmic_cpu_free(Cpu* cpu) { delete cpu; }
void kokoromi_dynarmic_set_watched(Cpu* cpu, bool watched) {
    if (!cpu->callbacks.guest_kernel) { if (cpu->using_fastmem == watched) cpu->rebuild(!watched); return; }
    if (cpu->callbacks.watched != watched) { cpu->callbacks.watched = watched; cpu->callbacks.invalidate(); cpu->rebuild(false); }
}
void kokoromi_dynarmic_set_guest_kernel(Cpu* cpu, bool enabled) {
    if (cpu->callbacks.guest_kernel == enabled) return;
    auto sp = cpu->jit->GetSP(); cpu->callbacks.guest_kernel = enabled; cpu->callbacks.current_el = 1;
    if (enabled) cpu->callbacks.page_table.assign(std::size_t{1} << (page_table_bits - 12), nullptr);
    cpu->callbacks.sp_el1 = enabled ? sp + memory_base : sp - memory_base; cpu->callbacks.invalidate(); cpu->rebuild(!enabled); cpu->jit->SetSP(cpu->callbacks.sp_el1);
    if (!enabled) { cpu->callbacks.page_table.clear(); cpu->callbacks.page_table.shrink_to_fit(); }
}
void kokoromi_dynarmic_set_interrupt_lines(Cpu* cpu, bool irq, bool fiq) { cpu->callbacks.irq = irq; cpu->callbacks.fiq = fiq; }
void kokoromi_dynarmic_set_virtual_time(Cpu* cpu, bool enabled) { cpu->callbacks.virtual_time = enabled; }
void kokoromi_dynarmic_set_tick_scale(Cpu* cpu, std::uint64_t scale) { cpu->callbacks.tick_scale = scale; }
std::uint64_t kokoromi_dynarmic_ticks(const Cpu* cpu) { return cpu->callbacks.now(); }
bool kokoromi_dynarmic_timer_pending(const Cpu* cpu) { return (cpu->callbacks.cntv_ctl & 3) == 1 && cpu->callbacks.now() >= cpu->callbacks.cntv_cval; }
bool kokoromi_dynarmic_idle_until(Cpu* cpu, std::uint64_t deadline) { if (cpu->callbacks.virtual_time) cpu->callbacks.ticks = std::max(cpu->callbacks.ticks, deadline); return cpu->callbacks.virtual_time; }
bool kokoromi_dynarmic_run(Cpu* cpu, std::uint64_t deadline, std::uint32_t core, bool* sliced) {
    cpu->callbacks.deadline = deadline; cpu->callbacks.svc = std::numeric_limits<std::uint32_t>::max(); cpu->callbacks.fault = false; cpu->callbacks.core = core;
    cpu->jit->ClearHalt(); cpu->callbacks.deliver_interrupts();
    if (cpu->callbacks.waiting && !cpu->callbacks.interrupt_pending() && !cpu->callbacks.fiq) {
        if (cpu->callbacks.virtual_time) {
            const auto idle = (cpu->callbacks.cntv_ctl & 3) == 1 && cpu->callbacks.cntv_cval <= deadline ? cpu->callbacks.cntv_cval : deadline;
            cpu->callbacks.ticks = std::max(cpu->callbacks.ticks, idle);
        }
        *sliced = true; return true;
    }
    Dynarmic::HaltReason reason{};
    for (;;) {
        if (cpu->callbacks.exception_pending) { cpu->jit->SetPC(cpu->callbacks.exception_pc); cpu->callbacks.exception_pending = false; }
        reason = cpu->jit->Run();
        if (cpu->callbacks.abort_pending) {
            cpu->callbacks.take_exception(cpu->callbacks.abort_esr, cpu->callbacks.abort_far, cpu->jit->GetPC(), 0);
            cpu->callbacks.abort_pending = false;
            continue;
        }
        if (Dynarmic::Has(reason, Dynarmic::HaltReason::CacheInvalidation)) continue;
        if (cpu->callbacks.resume) { cpu->callbacks.resume = false; cpu->callbacks.deadline = std::max(cpu->callbacks.deadline, cpu->callbacks.now() + 4 * cpu->callbacks.tick_scale); continue; }
        break;
    }
    *sliced = !cpu->callbacks.fault && cpu->callbacks.svc == std::numeric_limits<std::uint32_t>::max() && (!reason || cpu->callbacks.waiting);
    cpu->callbacks.deadline = std::numeric_limits<std::uint64_t>::max(); return !cpu->callbacks.fault;
}
bool kokoromi_dynarmic_step(Cpu* cpu) { cpu->callbacks.fault = false; cpu->jit->ClearHalt(); cpu->jit->Step(); return !cpu->callbacks.fault; }
void kokoromi_dynarmic_halt(Cpu* cpu) { cpu->jit->HaltExecution(); }
std::uint64_t kokoromi_dynarmic_get_x(const Cpu* cpu, std::size_t index) { return index < 31 ? cpu->jit->GetRegister(index) : 0; }
void kokoromi_dynarmic_set_x(Cpu* cpu, std::size_t index, std::uint64_t value) { if (index < 31) cpu->jit->SetRegister(index, value); }
std::uint64_t kokoromi_dynarmic_get_sp(const Cpu* cpu) { return cpu->jit->GetSP(); }
void kokoromi_dynarmic_set_sp(Cpu* cpu, std::uint64_t value) { cpu->jit->SetSP(value); }
std::uint64_t kokoromi_dynarmic_get_pc(const Cpu* cpu) { return cpu->jit->GetPC(); }
void kokoromi_dynarmic_set_pc(Cpu* cpu, std::uint64_t value) { cpu->jit->SetPC(value); }
void kokoromi_dynarmic_set_tls(Cpu* cpu, std::uint64_t value) { cpu->callbacks.tls = value; }
std::uint32_t kokoromi_dynarmic_take_svc(Cpu* cpu) { return std::exchange(cpu->callbacks.svc, std::numeric_limits<std::uint32_t>::max()); }
void kokoromi_dynarmic_clear_exclusive(Cpu* cpu) { cpu->jit->ClearExclusiveState(); cpu->monitor->value.ClearProcessor(cpu->id); }
void kokoromi_dynarmic_save(const Cpu* cpu, Context* context) { cpu->save(*context); }
void kokoromi_dynarmic_load(Cpu* cpu, const Context* context) { cpu->load(*context); }
void kokoromi_dynarmic_invalidate(Cpu* cpu, std::uint64_t address, std::size_t size) { cpu->jit->InvalidateCacheRange(address, size); }
}
