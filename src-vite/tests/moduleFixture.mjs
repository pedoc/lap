// Evaluate small dependency-injected source modules without Node's experimental
// vm.Module flag. Production modules are tested verbatim after TS transpilation.
import vm from 'node:vm';
import ts from 'typescript';

export function moduleFixture(source, dependencies, { context, globals = {}, meta = {} } = {}) {
  context ||= vm.createContext({ console, ...globals });
  const { outputText } = ts.transpileModule(source.replaceAll('import.meta', '__importMeta'), {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
  });
  const module = { exports: {} };
  const require = name => {
    if (!Object.hasOwn(dependencies, name)) throw new Error(`Unmocked fixture dependency: ${name}`);
    return dependencies[name];
  };
  vm.runInContext(`(function(exports, require, module, __importMeta) {\n${outputText}\n})`, context)(module.exports, require, module, meta);
  return { namespace: module.exports, context };
}
