; Windows installer for FM-1 emulator (Inno Setup). Built by .github/workflows/build.yml.
#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
[Setup]
AppId={{6E3B1F2A-8C4D-4E7A-9B21-FA11EC0FFEE1}
AppName=FM-1 emulator
AppVersion={#AppVersion}
AppPublisher=Transistor Labs
AppPublisherURL=https://ticcoross.gumroad.com/
DefaultDirName={commoncf64}\Transistor Labs\FM-1 emulator
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=FM-1-emulator-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
PrivilegesRequired=admin
LicenseFile=LICENSE.txt
WizardStyle=modern

[Files]
Source: "stage\FM-1.clap"; DestDir: "{commoncf64}\CLAP"; Flags: ignoreversion
Source: "stage\FM-1.vst3\*"; DestDir: "{commoncf64}\VST3\FM-1.vst3"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "LICENSE.txt"; DestDir: "{app}"; Flags: ignoreversion
