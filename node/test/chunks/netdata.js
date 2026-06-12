import assert from 'node:assert/strict';
import {
  NETDATA_SOURCE_TYPE_ALL,
  NETDATA_SOURCE_TYPE_LOCAL_ALL,
  NETDATA_SOURCE_TYPE_REMOTE_ALL,
  NETDATA_SOURCE_TYPE_LOCAL_SYSTEM,
  NETDATA_SOURCE_TYPE_LOCAL_USER,
  NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE,
  NETDATA_SOURCE_TYPE_LOCAL_OTHER,
  NETDATA_ACCEPTED_PARAMS,
  SYSTEMD_DEFAULT_VIEW_KEYS,
  SYSTEMD_DEFAULT_FACETS,
  DEFAULT_FUNCTION_NAME,
  DEFAULT_SOURCE_SELECTOR_NAME,
  DEFAULT_SOURCE_SELECTOR_HELP,
  DEFAULT_ITEMS_TO_RETURN,
  DEFAULT_TIME_WINDOW_SECONDS,
  DEFAULT_HISTOGRAM_BUCKETS,
  DisplayScope,
  DisplayContext,
  NetdataFunctionConfig,
  NetdataFunctionProfile,
  SystemdJournalProfile,
  SystemdJournalPluginProfile,
  priorityToRowSeverity,
} from '../../src/lib/netdata.js';

function testConstants() {
  assert.equal(NETDATA_SOURCE_TYPE_ALL, 1);
  assert.equal(NETDATA_SOURCE_TYPE_LOCAL_ALL, 2);
  assert.equal(NETDATA_SOURCE_TYPE_REMOTE_ALL, 4);
  assert.equal(NETDATA_SOURCE_TYPE_LOCAL_SYSTEM, 8);
  assert.equal(NETDATA_SOURCE_TYPE_LOCAL_USER, 16);
  assert.equal(NETDATA_SOURCE_TYPE_LOCAL_NAMESPACE, 32);
  assert.equal(NETDATA_SOURCE_TYPE_LOCAL_OTHER, 64);

  assert.equal(NETDATA_ACCEPTED_PARAMS.length, 16);
  assert.equal(NETDATA_ACCEPTED_PARAMS[0], 'info');
  assert.equal(NETDATA_ACCEPTED_PARAMS[15], 'slice');
}

function testViewKeysAndFacets() {
  assert.equal(SYSTEMD_DEFAULT_VIEW_KEYS.length, 22, 'view keys must be exactly 22');
  assert.equal(SYSTEMD_DEFAULT_VIEW_KEYS[0], '_HOSTNAME');
  assert.equal(SYSTEMD_DEFAULT_VIEW_KEYS[21], '_SOURCE_REALTIME_TIMESTAMP');

  assert.equal(SYSTEMD_DEFAULT_FACETS.length, 58, 'facets must be exactly 58');
  assert.equal(SYSTEMD_DEFAULT_FACETS[0], '_HOSTNAME');
  assert.equal(SYSTEMD_DEFAULT_FACETS[57], 'ND_ALERT_STATUS');

  const viewSet = new Set(SYSTEMD_DEFAULT_VIEW_KEYS);
  assert.equal(viewSet.size, 22, 'view keys must be unique');
  const facetSet = new Set(SYSTEMD_DEFAULT_FACETS);
  assert.equal(facetSet.size, 58, 'facets must be unique');
}

function testConfigDefaultsAndBackfill() {
  const cfg = NetdataFunctionConfig.systemdJournal();
  assert.equal(cfg.functionName, 'systemd-journal');
  assert.equal(cfg.sourceSelectorName, 'Journal Sources');
  assert.equal(cfg.sourceSelectorHelp, 'Select the logs source to query');
  assert.equal(cfg.defaultHistogram, 'PRIORITY');
  assert.deepEqual(cfg.defaultFacets, [...SYSTEMD_DEFAULT_FACETS]);
  assert.deepEqual(cfg.defaultViewKeys, [...SYSTEMD_DEFAULT_VIEW_KEYS]);
  assert.equal(cfg.readerOptions, null);
  assert.equal(cfg.explorerStrategy, null);

  const empty = new NetdataFunctionConfig({ sourceSelectorName: '', sourceSelectorHelp: '' });
  assert.equal(empty.sourceSelectorName, '');
  empty.backfillDefaults();
  assert.equal(empty.sourceSelectorName, DEFAULT_SOURCE_SELECTOR_NAME);
  assert.equal(empty.sourceSelectorHelp, DEFAULT_SOURCE_SELECTOR_HELP);

  const partial = new NetdataFunctionConfig({ sourceSelectorName: 'Custom' });
  partial.backfillDefaults();
  assert.equal(partial.sourceSelectorName, 'Custom');
  assert.equal(partial.sourceSelectorHelp, DEFAULT_SOURCE_SELECTOR_HELP);

  const chained = new NetdataFunctionConfig({ sourceSelectorName: '', sourceSelectorHelp: '' }).backfillDefaults();
  assert.ok(chained instanceof NetdataFunctionConfig);
}

function testDisplayScopeAndContext() {
  assert.equal(DisplayScope.Data, 'data');
  assert.equal(DisplayScope.Facet, 'facet');
  assert.equal(DisplayScope.Histogram, 'histogram');

  const ctx = new DisplayContext();
  assert.ok(ctx._bootFirstRealtime instanceof Map);
  assert.ok(ctx._uidCache instanceof Map);
  assert.ok(ctx._gidCache instanceof Map);
  assert.equal(ctx._bootFirstRealtime.size, 0);

  ctx.registerBootFirstRealtime('abc123', 1700000000000000n);
  assert.equal(ctx._bootFirstRealtime.get('abc123'), 1700000000000000n);
}

function testProfileTransformations() {
  const stdProfile = new SystemdJournalProfile();
  const pluginProfile = new SystemdJournalPluginProfile();
  const ctx = new DisplayContext();
  const enc = (s) => new TextEncoder().encode(s);

  // PRIORITY
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'PRIORITY', enc('3')), 'error');
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'PRIORITY', enc('0')), 'panic');
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'PRIORITY', enc('7')), 'debug');
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'PRIORITY', enc('99')), '99');

  // SYSLOG_FACILITY
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'SYSLOG_FACILITY', enc('1')), 'user');
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'SYSLOG_FACILITY', enc('23')), 'local7');
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'SYSLOG_FACILITY', enc('12')), '12');

  // ERRNO
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'ERRNO', enc('2')), '2 (ENOENT)');
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'ERRNO', enc('22')), '22 (EINVAL)');
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'ERRNO', enc('9999')), '9999');

  // _UID - standard profile returns raw; plugin profile returns raw (no resolution in Node)
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, '_UID', enc('0')), '0');
  assert.equal(pluginProfile.fieldDisplayValue(ctx, DisplayScope.Data, '_UID', enc('1000')), '1000');

  // _GID - same as _UID
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, '_GID', enc('0')), '0');
  assert.equal(pluginProfile.fieldDisplayValue(ctx, DisplayScope.Data, '_GID', enc('1000')), '1000');

  // _SYSTEMD_OWNER_UID
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, '_SYSTEMD_OWNER_UID', enc('42')), '42');
  assert.equal(pluginProfile.fieldDisplayValue(ctx, DisplayScope.Data, '_SYSTEMD_OWNER_UID', enc('42')), '42');

  // MESSAGE_ID - scope-aware
  const mid = 'f77379a8490b408bbe5f6940505a777b';
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'MESSAGE_ID', enc(mid)), `${mid} (Journal started)`);
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Facet, 'MESSAGE_ID', enc(mid)), 'Journal started');
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'MESSAGE_ID', enc('deadbeef')), 'deadbeef');

  // _BOOT_ID - without registered boot returns raw
  const bootId = '1234567890abcdef1234567890abcdef';
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, '_BOOT_ID', enc(bootId)), bootId);

  // _BOOT_ID - with registered boot
  const ctxWithBoot = new DisplayContext();
  ctxWithBoot.registerBootFirstRealtime(bootId, 1700000000000000);
  const bootDataResult = stdProfile.fieldDisplayValue(ctxWithBoot, DisplayScope.Data, '_BOOT_ID', enc(bootId));
  assert.ok(bootDataResult.startsWith(bootId + ' ('), `boot data result: ${bootDataResult}`);
  assert.ok(bootDataResult.endsWith('  '), 'boot data result must end with two spaces');
  const bootFacetResult = stdProfile.fieldDisplayValue(ctxWithBoot, DisplayScope.Facet, '_BOOT_ID', enc(bootId));
  assert.ok(!bootFacetResult.includes(bootId), `boot facet result should be timestamp only: ${bootFacetResult}`);
  assert.ok(bootFacetResult.endsWith('Z'), `boot facet result must end with Z: ${bootFacetResult}`);

  // _SOURCE_REALTIME_TIMESTAMP
  const rtResult = stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, '_SOURCE_REALTIME_TIMESTAMP', enc('1700000000123456'));
  assert.ok(rtResult.includes('1700000000123456'), `realtime result must include raw: ${rtResult}`);
  assert.ok(rtResult.includes('('), `realtime result must include parenthesized: ${rtResult}`);
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, '_SOURCE_REALTIME_TIMESTAMP', enc('0')), '0');

  // Unknown field: utf8-lossy passthrough
  assert.equal(stdProfile.fieldDisplayValue(ctx, DisplayScope.Data, 'UNKNOWN', enc('hello')), 'hello');
}

function testSeverityMapping() {
  assert.equal(priorityToRowSeverity(new TextEncoder().encode('0')), 'critical');
  assert.equal(priorityToRowSeverity(new TextEncoder().encode('1')), 'critical');
  assert.equal(priorityToRowSeverity(new TextEncoder().encode('3')), 'critical');
  assert.equal(priorityToRowSeverity(new TextEncoder().encode('4')), 'warning');
  assert.equal(priorityToRowSeverity(new TextEncoder().encode('5')), 'notice');
  assert.equal(priorityToRowSeverity(new TextEncoder().encode('6')), 'normal');
  assert.equal(priorityToRowSeverity(new TextEncoder().encode('7')), 'debug');
  assert.equal(priorityToRowSeverity(new TextEncoder().encode('8')), 'debug');
  assert.equal(priorityToRowSeverity(new TextEncoder().encode('abc')), 'normal');
}

function testRowOptions() {
  const profile = new SystemdJournalProfile();
  const enc = (s) => new TextEncoder().encode(s);

  const fieldsWithPriority = { PRIORITY: [enc('3')] };
  assert.deepEqual(profile.rowOptions(fieldsWithPriority), { severity: 'critical' });

  const fieldsNoPriority = { MESSAGE: [enc('hello')] };
  assert.deepEqual(profile.rowOptions(fieldsNoPriority), { severity: 'normal' });

  const fieldsEmpty = {};
  assert.deepEqual(profile.rowOptions(fieldsEmpty), { severity: 'normal' });
}

function testFacetOptionName() {
  const profile = new SystemdJournalProfile();
  const ctx = new DisplayContext();
  const enc = (s) => new TextEncoder().encode(s);

  assert.equal(profile.facetOptionName(ctx, 'PRIORITY', enc('3')), 'error');
  assert.equal(profile.facetOptionName(ctx, 'SYSLOG_FACILITY', enc('1')), 'user');
  assert.equal(profile.facetOptionName(ctx, 'UNKNOWN', enc('hello')), 'hello');
}

export async function run() {
  testConstants();
  testViewKeysAndFacets();
  testConfigDefaultsAndBackfill();
  testDisplayScopeAndContext();
  testProfileTransformations();
  testSeverityMapping();
  testRowOptions();
  testFacetOptionName();
  console.log('  PASS netdata foundation (chunk 2a)');
}
