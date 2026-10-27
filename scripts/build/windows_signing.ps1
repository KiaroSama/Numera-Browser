# Sign and verify owned executables without changing any machine trust store.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type -AssemblyName System.Security.Cryptography.Pkcs
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class NumeraAuthenticodeHash {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    public struct FileInfo {
        public uint Size; public string Path; public IntPtr File; public IntPtr Subject;
    }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    public struct TrustData {
        public uint Size; public IntPtr Policy; public IntPtr Client;
        public uint UI; public uint Revocation; public uint Choice; public IntPtr File;
        public uint Action; public IntPtr State; public string URL;
        public uint Flags; public uint UIContext; public IntPtr Settings;
    }
    [DllImport("wintrust.dll", ExactSpelling=true, PreserveSig=true)]
    private static extern int WinVerifyTrust(IntPtr window, ref Guid action, ref TrustData data);
    public static void Verify(string path) {
        FileInfo file = new FileInfo { Size=(uint)Marshal.SizeOf<FileInfo>(), Path=path };
        IntPtr memory=Marshal.AllocHGlobal(Marshal.SizeOf<FileInfo>());
        try {
            Marshal.StructureToPtr(file,memory,false);
            TrustData data=new TrustData { Size=(uint)Marshal.SizeOf<TrustData>(), UI=2,
                Choice=1, File=memory, Flags=0x200|0x1000|0x10, Action=1 };
            Guid action=new Guid("00AAC56B-CD44-11d0-8CC2-00C04FC295EE");
            int result;
            try { result=WinVerifyTrust(new IntPtr(-1),ref action,ref data); }
            finally { data.Action=2; WinVerifyTrust(new IntPtr(-1),ref action,ref data); }
            if(result!=0) throw new COMException("Authenticode file hash verification failed", result);
        } finally {
            Marshal.DestroyStructure<FileInfo>(memory); Marshal.FreeHGlobal(memory);
        }
    }
}
'@

function Verify-OwnedSignature([string]$Path, [string]$Fingerprint) {
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if (-not $signature.SignerCertificate) { throw 'Native executable has no Authenticode signer' }
    $actual = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($signature.SignerCertificate.RawData)).ToLowerInvariant()
    if ($actual -cne $Fingerprint) { throw 'Native executable signer differs from approved certificate' }
    # Private-root trust is intentionally not installed. Verify the Authenticode
    # file digest with Windows and separately authenticate the full CMS signature.
    [NumeraAuthenticodeHash]::Verify($Path)
    $stream = [IO.File]::OpenRead($Path)
    $reader = [IO.BinaryReader]::new($stream)
    try {
        $stream.Position = 0x3c
        $pe = $reader.ReadUInt32()
        $stream.Position = $pe + 24
        $magic = $reader.ReadUInt16()
        $entry = if ($magic -eq 0x20b) { $pe + 24 + 112 + 32 } elseif ($magic -eq 0x10b) { $pe + 24 + 96 + 32 } else { throw 'Unsupported PE header' }
        $stream.Position = $entry
        $offset = $reader.ReadUInt32()
        $size = $reader.ReadUInt32()
        if ($offset -le 0 -or $size -lt 8 -or $size -gt 1048576 -or $offset + $size -gt $stream.Length) { throw 'Invalid Authenticode certificate table' }
        $stream.Position = $offset
        $length = $reader.ReadUInt32()
        $revision = $reader.ReadUInt16()
        $type = $reader.ReadUInt16()
        if ($length -lt 8 -or $length -gt $size -or $revision -ne 0x200 -or $type -ne 2) { throw 'Unsupported Authenticode certificate record' }
        $cms = [Security.Cryptography.Pkcs.SignedCms]::new()
        $cms.Decode($reader.ReadBytes($length - 8))
        $cms.CheckSignature($true)
        if ($cms.SignerInfos.Count -ne 1) { throw 'Unexpected Authenticode signer count' }
        $cmsHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($cms.SignerInfos[0].Certificate.RawData)).ToLowerInvariant()
        if ($cmsHash -cne $Fingerprint) { throw 'CMS signer differs from approved certificate' }
    } finally { $reader.Dispose(); $stream.Dispose() }
}

$password = ConvertTo-SecureString $env:NUMERA_CODE_SIGNING_PASSWORD -AsPlainText -Force
$certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new(
    $env:NUMERA_SIGN_PFX, $password,
    [Security.Cryptography.X509Certificates.X509KeyStorageFlags]::EphemeralKeySet)
try {
    foreach ($relative in @('numera.exe', 'engine/numera-engine.exe')) {
        $path = Join-Path $env:NUMERA_SIGN_STAGE $relative
        $version = [Diagnostics.FileVersionInfo]::GetVersionInfo($path)
        if ($version.CompanyName -cne 'KiaroSama' -or $version.ProductName -cne 'Numera Browser' -or
            $version.FileVersion -cne $env:NUMERA_SIGN_VERSION -or $version.ProductVersion -cne $env:NUMERA_SIGN_VERSION) {
            throw 'Native executable embedded product identity differs from the approved build'
        }
        $signed = Set-AuthenticodeSignature -LiteralPath $path -Certificate $certificate -HashAlgorithm SHA256 -IncludeChain Signer
        if (-not $signed.SignerCertificate) { throw 'Authenticode signing operation produced no signer' }
        Verify-OwnedSignature $path $env:NUMERA_SIGN_FINGERPRINT
        $prefix = if ($env:NUMERA_SIGN_FINGERPRINT[0] -eq '0') { '1' } else { '0' }
        $wrong = $prefix + $env:NUMERA_SIGN_FINGERPRINT.Substring(1)
        $rejected = $false
        try { Verify-OwnedSignature $path $wrong } catch {
            if ($_.Exception.Message -cne 'Native executable signer differs from approved certificate') { throw }
            $rejected = $true
        }
        if (-not $rejected) { throw 'Wrong Authenticode signer was accepted' }
        Verify-OwnedSignature $path $env:NUMERA_SIGN_FINGERPRINT
        # The safe copy must actually fail verification after a covered byte is
        # changed. The original product is never modified by this rejection case.
        $copy = Join-Path (Split-Path $env:NUMERA_SIGN_PFX) 'tampered.exe'
        [IO.File]::Copy($path, $copy, $true)
        try {
            $bytes = [IO.File]::ReadAllBytes($copy)
            $bytes[64] = $bytes[64] -bxor 1
            [IO.File]::WriteAllBytes($copy, $bytes)
            $rejected = $false
            try { Verify-OwnedSignature $copy $env:NUMERA_SIGN_FINGERPRINT } catch {
                # TRUST_E_BAD_DIGEST is evidence of tampering; other failures are not.
                if ($_.Exception.GetBaseException().HResult -ne -2146869232) { throw }
                $rejected = $true
            }
            if (-not $rejected) { throw 'Tampered Authenticode copy was accepted' }
            Verify-OwnedSignature $path $env:NUMERA_SIGN_FINGERPRINT
        } finally { Remove-Item -LiteralPath $copy -Force -ErrorAction SilentlyContinue }
    }
    Write-Output 'Owned Authenticode signatures verified; private trust, no OS trust installation.'
} finally { $certificate.Dispose() }
