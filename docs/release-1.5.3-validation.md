# ProjectPane 1.5.3 validation

This patch adds root-type badges and persistent project-tab ordering. Existing manifest arrays retain their order without migration. Reordering does not move files or change the active project. Full native, package and publication results will be recorded before completion.

Root classification uses path syntax for UNC/WSL and GetDriveTypeW for drive letters, including mapped network drives. Unrecognized drive types remain unknown. It does not establish network availability or permissions.
