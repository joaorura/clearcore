#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dlfcn.h>
#include <stdint.h>
#include <time.h>

typedef void* (*create_fn_t)(const char*);
typedef int (*process_fn_t)(void*, const float*, float*);
typedef void (*free_fn_t)(void*);

static double get_time_sec(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (double)ts.tv_sec + (double)ts.tv_nsec / 1e9;
}

int main(int argc, char *argv[]) {
    if (argc < 3) {
        printf("Usage: %s <input.wav> <output.wav>\n", argv[0]);
        return 1;
    }

    const char *in_path = argv[1];
    const char *out_path = argv[2];

    FILE *fin = fopen(in_path, "rb");
    if (!fin) {
        fprintf(stderr, "Cannot open %s\n", in_path);
        return 1;
    }

    char chunk_id[4];
    uint32_t chunk_size = 0;

    if (fread(chunk_id, 1, 4, fin) != 4 || memcmp(chunk_id, "RIFF", 4) != 0) {
        fprintf(stderr, "Not a RIFF file\n");
        fclose(fin);
        return 1;
    }

    uint32_t file_size;
    fread(&file_size, 4, 1, fin);

    char wave_id[4];
    fread(wave_id, 1, 4, fin);
    if (memcmp(wave_id, "WAVE", 4) != 0) {
        fprintf(stderr, "Not a WAVE file\n");
        fclose(fin);
        return 1;
    }

    uint16_t channels = 0;
    uint32_t sample_rate = 0;
    uint16_t bits_per_sample = 0;
    long data_pos = -1;
    uint32_t data_bytes = 0;

    while (fread(chunk_id, 1, 4, fin) == 4 && fread(&chunk_size, 4, 1, fin) == 1) {
        if (memcmp(chunk_id, "fmt ", 4) == 0) {
            uint16_t audio_fmt;
            fread(&audio_fmt, 2, 1, fin);
            fread(&channels, 2, 1, fin);
            fread(&sample_rate, 4, 1, fin);
            uint32_t byte_rate;
            uint16_t block_align;
            fread(&byte_rate, 4, 1, fin);
            fread(&block_align, 2, 1, fin);
            fread(&bits_per_sample, 2, 1, fin);
            if (chunk_size > 16) {
                fseek(fin, chunk_size - 16, SEEK_CUR);
            }
        } else if (memcmp(chunk_id, "data", 4) == 0) {
            data_pos = ftell(fin);
            data_bytes = chunk_size;
            break;
        } else {
            fseek(fin, chunk_size, SEEK_CUR);
        }
    }

    if (data_pos < 0 || channels != 1 || sample_rate != 48000 || bits_per_sample != 16) {
        fprintf(stderr, "Expected 48kHz mono 16-bit PCM (ch=%d, rate=%d, bits=%d, data_bytes=%u)\n",
                channels, sample_rate, bits_per_sample, data_bytes);
        fclose(fin);
        return 1;
    }

    fseek(fin, data_pos, SEEK_SET);
    size_t n_samples = data_bytes / sizeof(int16_t);
    int16_t *pcm_in = malloc(n_samples * sizeof(int16_t));
    if (fread(pcm_in, sizeof(int16_t), n_samples, fin) != n_samples) {
        fprintf(stderr, "Failed to read PCM data\n");
        fclose(fin);
        free(pcm_in);
        return 1;
    }
    fclose(fin);

    /* Load ClearCore neural filter */
    void *lib = dlopen("./platform/linux/helper/lib/libclearcore_filter.so", RTLD_NOW | RTLD_GLOBAL);
    if (!lib) {
        fprintf(stderr, "dlopen failed: %s\n", dlerror());
        free(pcm_in);
        return 1;
    }

    create_fn_t create_fn = (create_fn_t)dlsym(lib, "clearcore_filter_create");
    process_fn_t process_fn = (process_fn_t)dlsym(lib, "clearcore_filter_process");
    free_fn_t free_fn = (free_fn_t)dlsym(lib, "clearcore_filter_free");

    void *filter = create_fn(".");
    if (!filter) {
        fprintf(stderr, "Failed to create filter\n");
        free(pcm_in);
        dlclose(lib);
        return 1;
    }

    int16_t *pcm_out = malloc(n_samples * sizeof(int16_t));
    float in_frame[480];
    float out_frame[480];

    printf("[ClearCore Native Engine] Denoising %zu samples (%.2f seconds of 48kHz audio)...\n",
           n_samples, (double)n_samples / 48000.0);

    double t0 = get_time_sec();
    for (size_t offset = 0; offset < n_samples; offset += 480) {
        size_t count = (offset + 480 <= n_samples) ? 480 : (n_samples - offset);
        for (size_t i = 0; i < count; i++) {
            in_frame[i] = (float)pcm_in[offset + i] / 32768.0f;
        }
        for (size_t i = count; i < 480; i++) {
            in_frame[i] = 0.0f;
        }

        process_fn(filter, in_frame, out_frame);

        for (size_t i = 0; i < count; i++) {
            float s = out_frame[i];
            if (s > 1.0f) s = 1.0f;
            if (s < -1.0f) s = -1.0f;
            pcm_out[offset + i] = (int16_t)(s * 32767.0f);
        }
    }
    double elapsed = get_time_sec() - t0;
    printf("[ClearCore Native Engine] Processed %.2f seconds in %.3f seconds (%.1fx faster than real-time)!\n",
           (double)n_samples / 48000.0, elapsed, ((double)n_samples / 48000.0) / elapsed);

    /* Write standard WAV output */
    FILE *fout = fopen(out_path, "wb");
    if (!fout) {
        fprintf(stderr, "Cannot open %s for write\n", out_path);
        free_fn(filter);
        dlclose(lib);
        free(pcm_in);
        free(pcm_out);
        return 1;
    }

    uint32_t out_data_bytes = (uint32_t)(n_samples * sizeof(int16_t));
    uint32_t out_riff_size = 36 + out_data_bytes;
    fwrite("RIFF", 1, 4, fout);
    fwrite(&out_riff_size, 4, 1, fout);
    fwrite("WAVE", 1, 4, fout);
    fwrite("fmt ", 1, 4, fout);
    uint32_t fmt_size = 16;
    fwrite(&fmt_size, 4, 1, fout);
    uint16_t format_tag = 1; /* PCM */
    fwrite(&format_tag, 2, 1, fout);
    fwrite(&channels, 2, 1, fout);
    fwrite(&sample_rate, 4, 1, fout);
    uint32_t byte_rate = sample_rate * channels * 2;
    fwrite(&byte_rate, 4, 1, fout);
    uint16_t block_align = channels * 2;
    fwrite(&block_align, 2, 1, fout);
    fwrite(&bits_per_sample, 2, 1, fout);
    fwrite("data", 1, 4, fout);
    fwrite(&out_data_bytes, 4, 1, fout);
    fwrite(pcm_out, sizeof(int16_t), n_samples, fout);
    fclose(fout);

    free_fn(filter);
    dlclose(lib);
    free(pcm_in);
    free(pcm_out);

    printf("[ClearCore Native Engine] Denoised audio saved to: %s\n", out_path);
    return 0;
}
