import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const outputDirectory = process.argv[2] ?? 'build/release-metadata';
const packageLock = JSON.parse(readFileSync('package-lock.json', 'utf8'));
const cargoMetadata = JSON.parse(
  execFileSync(
    'cargo',
    [
      'metadata',
      '--format-version',
      '1',
      '--locked',
      '--manifest-path',
      'src-tauri/Cargo.toml',
    ],
    { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 },
  ),
);

const npmComponents = Object.entries(packageLock.packages)
  .filter(
    ([path, metadata]) => path.includes('node_modules/') && metadata.version,
  )
  .map(([path, metadata]) => ({
    ecosystem: 'npm',
    name: path.slice(
      path.lastIndexOf('node_modules/') + 'node_modules/'.length,
    ),
    version: metadata.version,
    license: metadata.license ?? 'UNKNOWN',
    source: metadata.resolved ?? null,
  }));

const rustComponents = cargoMetadata.packages
  .filter((metadata) => metadata.source !== null)
  .map((metadata) => ({
    ecosystem: 'cargo',
    name: metadata.name,
    version: metadata.version,
    license: metadata.license ?? 'UNKNOWN',
    source: metadata.source,
  }));

const components = [...npmComponents, ...rustComponents].sort((left, right) =>
  `${left.ecosystem}:${left.name}:${left.version}`.localeCompare(
    `${right.ecosystem}:${right.name}:${right.version}`,
  ),
);

const report = {
  project: packageLock.name,
  version: packageLock.version,
  notice:
    "This machine-generated inventory reports dependency-declared licenses. Refer to each package's source distribution for authoritative license terms.",
  components,
};

mkdirSync(outputDirectory, { recursive: true });
writeFileSync(
  join(outputDirectory, 'THIRD-PARTY-LICENSES.json'),
  `${JSON.stringify(report, null, 2)}\n`,
);
