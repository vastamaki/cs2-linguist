Vendored from crates.io `wasapi` 0.24.0 (MIT, see LICENSE.txt).

The upstream `AudioCaptureClient` read methods dereference the packet pointer even
when `AUDCLNT_BUFFERFLAGS_SILENT` is set. Windows documents that pointer as undefined
for silent packets. Both read methods now materialize zeroes without dereferencing
it. The deque reader also propagates ReleaseBuffer errors instead of panicking.

These are the only source changes. Remove the patch when an upstream release includes
equivalent handling. The upstream examples and development-only files and manifest
targets are omitted.

Reference: https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer
