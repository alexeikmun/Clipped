
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const projectRoot = path.resolve(__dirname, '..');
const sourceDir = path.join(projectRoot, 'src-tauri', 'target', 'release', 'bundle', 'nsis');
const destDir = path.join(projectRoot, 'releases', 'download');

// Ensure destination directory exists
if (!fs.existsSync(destDir)) {
    fs.mkdirSync(destDir, { recursive: true });
    console.log(`Created directory: ${destDir}`);
}

// Check if source directory exists
if (!fs.existsSync(sourceDir)) {
    console.error(`Source directory not found: ${sourceDir}`);
    console.error('Make sure to run "tauri build" before this script.');
    process.exit(1);
}

// Read current version from package.json
const pkgPath = path.join(projectRoot, 'package.json');
const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
const currentVersion = pkg.version;

// Read files from source directory matching current version
const files = fs.readdirSync(sourceDir);
const exeFiles = files.filter(file => file.endsWith('.exe') && file.includes(`_${currentVersion}_`));

if (exeFiles.length === 0) {
    console.log(`No .exe files found matching version ${currentVersion} to copy.`);
    process.exit(0);
}

// Copy each .exe file
exeFiles.forEach(file => {
    const srcPath = path.join(sourceDir, file);
    const destPath = path.join(destDir, file);
    
    fs.copyFileSync(srcPath, destPath);
    console.log(`Copied ${file} to ${destDir}`);
});
