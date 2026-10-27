# The WiX Toolset the MSI is built with (03-package.ps1), installed by
# 00-install-prerequisites.ps1 and detected by 01-configure.ps1. Unlike the GPU
# dists (dist-sources.psd1, never pinned), this IS pinned: the .wxs is written
# against one schema and one WixUI dialog set, and a new major has changed
# both before (v3 to v4) and its licensing (v7). Moving to a new version is a
# deliberate edit here, followed by a package build and the ICE validation
# 03-package.ps1 runs.
@{
    # `dotnet tool install --global wix --version <Version>`. Needs the .NET SDK
    # (DotNetSdkWingetId below), the only prerequisite of the tool itself.
    Version = '7.0.1'

    # Extensions the .wxs uses, at the SAME version as the tool (WiX refuses a
    # mismatched major): WixUI for the dialogs (CustomizeDlg, MaintenanceTypeDlg,
    # ProgressDlg, ...), Util for WixQuietExec, which runs the custom actions'
    # PowerShell without a console window.
    Extensions = @('WixToolset.UI.wixext', 'WixToolset.Util.wixext')

    # WiX v7 is under the Open Source Maintenance Fee EULA: every command but
    # `--version` and `eula` fails with WIX7015 until it is accepted once per
    # user (`wix eula accept wix7`). The scripts never accept it on the user's
    # behalf; they stop and say how. (Organizations with more than USD 10,000
    # yearly revenue owe the maintenance fee: https://opensourcemaintenancefee.org)
    EulaId = 'wix7'

    # winget id of the .NET SDK that hosts the wix tool.
    DotNetSdkWingetId = 'Microsoft.DotNet.SDK.10'
}
