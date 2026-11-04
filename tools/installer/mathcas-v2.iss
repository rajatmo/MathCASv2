; MathCAS v2 installer (Inno Setup 6). Built by tools\package.ps1, which passes:
;   /DAppVersion=<commit> /DSourceDir=<dist folder> /DOutputDir=<packages folder> /DOutputName=<file name>
; Per-user by default (no administrator rights), all users on request; Start Menu entries; optional PATH entry
; (removed again on uninstall); optional VS Code extension on the last page.

#ifndef AppVersion
  #define AppVersion "dev"
#endif

[Setup]
AppId={{C366AAA7-AA93-4378-9EAC-0D9B8E37C835}
AppName=MathCAS v2
AppVersion={#AppVersion}
AppVerName=MathCAS v2 ({#AppVersion})
AppPublisher=MathCAS project
DefaultDirName={autopf}\MathCAS v2
DefaultGroupName=MathCAS v2
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesEnvironment=yes
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\icon.ico
UninstallDisplayName=MathCAS v2
SourceDir={#SourceDir}
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "addtopath"; Description: "Add MathCAS to the PATH (mc-repl, mc-kernel and mc-lsp in any terminal)"

[Files]
Source: "*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\MathCAS v2 REPL"; Filename: "{app}\mc-repl.exe"; WorkingDir: "{userdocs}"; IconFilename: "{app}\icon.ico"
Name: "{group}\MathCAS v2 quick start guide"; Filename: "{app}\quickstart.pdf"
Name: "{group}\MathCAS v2 examples"; Filename: "{app}\examples"
Name: "{group}\Uninstall MathCAS v2"; Filename: "{uninstallexe}"

[Run]
Filename: "{cmd}"; Parameters: "/c code --install-extension ""{app}\mathcas-0.1.0.vsix"""; \
  Description: "Install the MathCAS extension into VS Code"; Check: VsCodeFound; \
  Flags: postinstall unchecked skipifsilent runhidden
Filename: "{cmd}"; Parameters: "/c python ""{app}\jupyter\install.py"" --kernel ""{app}\mc-kernel.exe"""; \
  Description: "Register the MathCAS kernel with Jupyter (notebooks in Jupyter and VS Code)"; Check: JupyterFound; \
  Flags: postinstall skipifsilent runhidden
Filename: "{app}\quickstart.pdf"; Description: "Open the quick start guide"; \
  Flags: postinstall shellexec skipifsilent unchecked

[UninstallRun]
Filename: "{cmd}"; Parameters: "/c code --uninstall-extension mathcas.mathcas"; Check: VsCodeFound; \
  Flags: runhidden; RunOnceId: "VsCodeExtension"
Filename: "{cmd}"; Parameters: "/c python ""{app}\jupyter\install.py"" --uninstall"; Check: JupyterFound; \
  Flags: runhidden; RunOnceId: "JupyterKernel"

[Code]
const
  EnvKeyUser = 'Environment';
  EnvKeyMachine = 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment';

function VsCodeFound: Boolean;
var
  Code: Integer;
begin
  Result := Exec(ExpandConstant('{cmd}'), '/c where code >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, Code)
    and (Code = 0);
end;

{ a Python on the PATH with ipykernel: what the Jupyter kernel needs }
function JupyterFound: Boolean;
var
  Code: Integer;
begin
  Result := Exec(ExpandConstant('{cmd}'), '/c python -c "import ipykernel, jupyter_client" >nul 2>nul', '',
    SW_HIDE, ewWaitUntilTerminated, Code) and (Code = 0);
end;

{ the PATH of this install's scope: the user's for a per-user install, the machine's for all users }
procedure PathKey(var Root: Integer; var Key: String);
begin
  if IsAdminInstallMode then begin Root := HKEY_LOCAL_MACHINE; Key := EnvKeyMachine; end
  else begin Root := HKEY_CURRENT_USER; Key := EnvKeyUser; end;
end;

function HasEntry(const Path, Dir: String): Boolean;
begin
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Path) + ';') > 0;
end;

procedure AddToPath(const Dir: String);
var
  Root: Integer; Key, Path: String;
begin
  PathKey(Root, Key);
  if not RegQueryStringValue(Root, Key, 'Path', Path) then Path := '';
  if HasEntry(Path, Dir) then exit;
  if (Path <> '') and (Copy(Path, Length(Path), 1) <> ';') then Path := Path + ';';
  RegWriteExpandStringValue(Root, Key, 'Path', Path + Dir);
end;

procedure RemoveFromPath(const Dir: String);
var
  Root: Integer; Key, Path, Rest, Part, Kept: String; P: Integer;
begin
  PathKey(Root, Key);
  if not RegQueryStringValue(Root, Key, 'Path', Path) then exit;
  Rest := Path + ';'; Kept := '';
  while Rest <> '' do begin
    P := Pos(';', Rest);
    Part := Copy(Rest, 1, P - 1);
    Delete(Rest, 1, P);
    if (Part <> '') and (CompareText(Part, Dir) <> 0) then begin
      if Kept <> '' then Kept := Kept + ';';
      Kept := Kept + Part;
    end;
  end;
  if Kept <> Path then RegWriteExpandStringValue(Root, Key, 'Path', Kept);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('addtopath') then AddToPath(ExpandConstant('{app}'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then RemoveFromPath(ExpandConstant('{app}'));
end;
