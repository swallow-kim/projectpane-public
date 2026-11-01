# Trusted program, JSON data on stdin. No user path is evaluated as PowerShell.
$ErrorActionPreference = 'Stop'
try {
    $request = [System.IO.StreamReader]::new([Console]::OpenStandardInput(), [System.Text.UTF8Encoding]::new($false)).ReadToEnd() | ConvertFrom-Json
    if ($null -eq $request -or $null -eq $request.extract) { throw 'missing_request' }
    Add-Type -AssemblyName System.IO.Compression
    $output = [Console]::OpenStandardOutput()
    if ($request.extract) {
        $sourceStream = [System.IO.File]::OpenRead($request.source)
        try {
            $zip = [System.IO.Compression.ZipArchive]::new($sourceStream, [System.IO.Compression.ZipArchiveMode]::Read, $true)
            try {
                # Rust has already bounded and validated the central directory.
                if ($zip.Entries.Count -ne $request.count) { throw 'entry_count' }
                foreach ($entry in $zip.Entries) {
                    $stream = $entry.Open()
                    try { $stream.CopyTo($output, 65536) } finally { $stream.Dispose() }
                }
            } finally { $zip.Dispose() }
        } finally { $sourceStream.Dispose() }
    } else {
        # .NET Framework ZipArchive queries Position even on non-seekable output.
        # Count bytes without buffering the archive or pretending it is seekable.
        Add-Type -TypeDefinition @'
using System;
using System.IO;
public sealed class ProjectPaneZipOutput : Stream {
    private readonly Stream output;
    private long position;
    public ProjectPaneZipOutput(Stream value) { output = value; }
    public override bool CanRead { get { return false; } }
    public override bool CanSeek { get { return false; } }
    public override bool CanWrite { get { return true; } }
    public override long Length { get { return position; } }
    public override long Position { get { return position; } set { throw new NotSupportedException(); } }
    public override void Flush() { output.Flush(); }
    public override void Write(byte[] buffer, int offset, int count) { output.Write(buffer, offset, count); position += count; }
    public override int Read(byte[] buffer, int offset, int count) { throw new NotSupportedException(); }
    public override long Seek(long value, SeekOrigin origin) { throw new NotSupportedException(); }
    public override void SetLength(long value) { throw new NotSupportedException(); }
}
'@
        $output = [ProjectPaneZipOutput]::new($output)
        $zip = [System.IO.Compression.ZipArchive]::new($output, [System.IO.Compression.ZipArchiveMode]::Create, $true, [System.Text.Encoding]::UTF8)
        try {
            foreach ($item in $request.items) {
                $entry = $zip.CreateEntry($item.name, [System.IO.Compression.CompressionLevel]::Optimal)
                if (!$item.folder) {
                    $sourceStream = [System.IO.File]::OpenRead($item.path)
                    try {
                        $stream = $entry.Open()
                        try { $sourceStream.CopyTo($stream, 65536) } finally { $stream.Dispose() }
                    } finally { $sourceStream.Dispose() }
                }
            }
        } finally { $zip.Dispose() }
    }
    $output.Flush()
    exit 0
} catch { [Console]::Error.WriteLine($_.Exception.ToString()); exit 1 }
