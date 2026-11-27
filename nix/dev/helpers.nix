{pkgs}: let
  inherit (pkgs) lib;

  step = name: text: pkgs.writeShellApplication {inherit name text;};

  out = name: "$GIT_ROOT/dist/${name}";

  # Folders that builds write into and that sit inside watched source
  # folders. They are excluded so that a build does not retrigger itself.
  written = [
    "generated"
    "**/generated/**"
    "dist"
    "**/dist/**"
    "node_modules"
    "**/node_modules/**"
  ];

  filesOf = package: let
    # A `workspace.sourceFor` source is a derivation, which cannot be listed
    # without building it. It carries the plain source it was made from as
    # `files`.
    src = toString (package.src.files or package.src);
  in
    # The paths become attribute names in `sourcesOf`, and attribute names
    # cannot carry a string context.
    map (file: builtins.unsafeDiscardStringContext (lib.removePrefix "${src}/" (toString file))) (
      lib.filesystem.listFilesRecursive src
    );

  # There is one `watch.paths` entry per folder with a filter on file
  # extensions, so that a new source file is picked up without restarting.
  sourcesOf = packages: let
    kind = file: let
      extension = builtins.match ".*(\\.[^./]+)" (baseNameOf file);
    in
      if extension == null
      then baseNameOf file
      else "*${builtins.head extension}";
  in
    lib.concatLists (
      lib.mapAttrsToList (
        folder: files:
        # A folder entry for the repository root would watch the whole tree,
        # so files at the root are listed one by one.
          if folder == "."
          then map (path: {inherit path;}) files
          else [
            {
              path = folder;
              include = lib.unique (map kind files);
              exclude = written;
            }
          ]
      ) (lib.groupBy dirOf (lib.unique (lib.concatMap filesOf packages)))
    );

  # Waits for the first path to exist, because the build that creates it may
  # still be running. --no-vcs-ignore is needed because the watched paths are
  # gitignored.
  restartOn = paths: command: ''
    until [ -e "${builtins.head paths}" ]; do sleep 1; done
    exec ${lib.getExe pkgs.watchexec} --restart --no-vcs-ignore --debounce 500ms \
      ${lib.concatMapStringsSep " " (path: "--watch \"${path}\"") paths} -- ${command}
  '';

  linked = to: name: ''
    if [ "$(readlink ${name} 2> /dev/null)" != "${to}" ]; then
      rm -rf ${name}
      mkdir -p "$(dirname ${name})"
      ln -s "${to}" ${name}
    fi
  '';

  # Copies only if `from` and `to` differ, so that an unchanged output does
  # not restart whatever watches it. A file is removed before it is copied
  # because a running binary cannot be overwritten in place. rsync renames
  # each file of a folder into place and deletes stale ones last, so a watcher
  # such as rspack's never sees one missing or half written.
  publish = from: to: ''
    if ! ${pkgs.diffutils}/bin/diff -rq "${from}" "${to}" > /dev/null 2>&1; then
      if [ -d "${from}" ]; then
        mkdir -p "${to}"
        ${pkgs.rsync}/bin/rsync -r --ignore-times --delete-delay "${from}"/ "${to}"/
      else
        mkdir -p "${dirOf to}"
        rm -f "${to}"
        cp "${from}" "${to}"
      fi
    fi
  '';

  built = name: {
    command,
    from,
    to,
    packages,
  }: {
    "${name}-build" = {
      command = step "${name}-build" ''
        ${command}
        ${publish from to}
      '';

      # A run of saves, as a search and replace or an agent's edits make, is
      # one restart. process-compose stops watching a process for good once
      # it restarts 10 times in a minute, taking it for one that triggers
      # itself.
      watch = {
        paths = sourcesOf packages;
        debounce = "2s";
      };
    };
  };

  started.condition = "process_started";
  completed.condition = "process_completed_successfully";
in {
  inherit
    step
    out
    sourcesOf
    restartOn
    linked
    publish
    built
    started
    completed
    ;
}
