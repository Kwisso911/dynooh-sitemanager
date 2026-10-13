<?php
/**
 * Dynooh SiteManager – sends the one-time code for resetting the master password.
 * Upload to https://apps.dynooh.com/api/reset-mail.php (URL is compiled into the app).
 *
 * Delivery order: direct to the recipient's MX, then optional SMTP login (mail.json outside the web root),
 * then PHP mail(). The SPF record of dynooh.com must allow this server's IP.
 */

/** reset-mail.config.php (not in the repository): <?php return ['token' => '…']; */
define('TOKEN', (string)((require __DIR__ . '/reset-mail.config.php')['token'] ?? ''));
const FROM_ADDR = 'noreply@dynooh.com';
const FROM_NAME = 'Dynooh SiteManager';
const HELO = 'dynooh.com';
const LIMIT_PER_EMAIL = 3;   // per hour
const LIMIT_PER_IP = 10;     // per hour

ini_set('display_errors', '0');
header('Content-Type: application/json; charset=utf-8');

function reply(int $status, array $body): void
{
    http_response_code($status);
    echo json_encode($body);
    exit;
}

if ($_SERVER['REQUEST_METHOD'] !== 'POST') {
    reply(405, ['error' => 'method']);
}
if (TOKEN === '' || !hash_equals(TOKEN, $_SERVER['HTTP_X_VF_TOKEN'] ?? '')) {
    reply(403, ['error' => 'token']);
}

$in = json_decode(file_get_contents('php://input'), true);
$email = trim((string)($in['email'] ?? ''));
$code = (string)($in['code'] ?? '');
$lang = ($in['lang'] ?? 'en') === 'de' ? 'de' : 'en';

if (!filter_var($email, FILTER_VALIDATE_EMAIL) || preg_match('/[\r\n<>]/', $email) || !preg_match('/^\d{6}$/', $code)) {
    reply(400, ['error' => 'input']);
}

/** Counts requests per key within the last hour; returns false once the limit is reached. */
function allow(string $key, int $limit): bool
{
    $file = sys_get_temp_dir() . '/dynooh-sm-' . hash('sha256', $key);
    $now = time();
    $hits = array_filter(
        array_map('intval', @file($file, FILE_IGNORE_NEW_LINES) ?: []),
        fn($t) => $t > $now - 3600
    );
    if (count($hits) >= $limit) {
        return false;
    }
    $hits[] = $now;
    file_put_contents($file, implode("\n", $hits), LOCK_EX);
    return true;
}

/** Optional SMTP login: {"host":"…","port":587,"user":"…","pass":"…"} in the vhost's private folder. */
function mail_config(): ?array
{
    $path = dirname(__DIR__, 2) . '/private/vfsm-mail.json';
    if (!is_readable($path)) {
        return null;
    }
    $data = json_decode((string)file_get_contents($path), true);
    return is_array($data) && !empty($data['host']) && !empty($data['user']) && !empty($data['pass']) ? $data : null;
}

function smtp_read($fp): string
{
    $out = '';
    while (!feof($fp)) {
        $line = fgets($fp, 512);
        if ($line === false) {
            break;
        }
        $out .= $line;
        if (isset($line[3]) && $line[3] === ' ') {
            break;
        }
    }
    return $out;
}

function smtp_cmd($fp, string $cmd): string
{
    fwrite($fp, $cmd . "\r\n");
    return smtp_read($fp);
}

function smtp_ok(string $resp, string $prefix = '2'): bool
{
    return strncmp($resp, $prefix, strlen($prefix)) === 0;
}

function smtp_connect(string $target)
{
    $ctx = stream_context_create(['ssl' => ['verify_peer' => false, 'verify_peer_name' => false, 'allow_self_signed' => true]]);
    $fp = @stream_socket_client($target, $errno, $errstr, 15, STREAM_CLIENT_CONNECT, $ctx);
    if (!$fp) {
        return null;
    }
    stream_set_timeout($fp, 20);
    return $fp;
}

function smtp_starttls($fp, bool $required): bool
{
    if (!smtp_ok(smtp_cmd($fp, 'STARTTLS'))) {
        return !$required;
    }
    return @stream_socket_enable_crypto($fp, true, STREAM_CRYPTO_METHOD_TLS_CLIENT)
        && smtp_ok(smtp_cmd($fp, 'EHLO ' . HELO));
}

function encoded_subject(string $subject): string
{
    return '=?UTF-8?B?' . base64_encode($subject) . '?=';
}

function payload(string $to, string $subject, string $body): string
{
    $safe = preg_replace('/^\./m', '..', str_replace(["\r\n", "\r"], "\n", $body));
    return implode("\r\n", [
        'From: ' . FROM_NAME . ' <' . FROM_ADDR . '>',
        'To: <' . $to . '>',
        'Subject: ' . encoded_subject($subject),
        'Date: ' . date('r'),
        'Message-ID: <' . bin2hex(random_bytes(8)) . '@' . HELO . '>',
        'MIME-Version: 1.0',
        'Content-Type: text/plain; charset=UTF-8',
        'Content-Transfer-Encoding: 8bit',
        '',
        str_replace("\n", "\r\n", (string)$safe),
        '.',
    ]);
}

function smtp_send($fp, string $to, string $subject, string $body): bool
{
    if (!smtp_ok(smtp_cmd($fp, 'MAIL FROM:<' . FROM_ADDR . '>'))
        || !smtp_ok(smtp_cmd($fp, 'RCPT TO:<' . $to . '>'))
        || !smtp_ok(smtp_cmd($fp, 'DATA'), '3')
    ) {
        return false;
    }
    fwrite($fp, payload($to, $subject, $body) . "\r\n");
    return smtp_ok(smtp_read($fp));
}

function recipient_mx(string $email): string
{
    $domain = substr($email, strrpos($email, '@') + 1);
    $hosts = [];
    $weights = [];
    if (function_exists('getmxrr') && getmxrr($domain, $hosts, $weights) && $hosts) {
        array_multisort($weights, SORT_NUMERIC, $hosts);
        return (string)$hosts[0];
    }
    return $domain;
}

function send_mx(string $to, string $subject, string $body): bool
{
    $fp = smtp_connect('tcp://' . recipient_mx($to) . ':25');
    if (!$fp) {
        return false;
    }
    $ok = smtp_ok(smtp_read($fp))
        && smtp_ok(smtp_cmd($fp, 'EHLO ' . HELO))
        && smtp_starttls($fp, false)
        && smtp_send($fp, $to, $subject, $body);
    smtp_cmd($fp, 'QUIT');
    fclose($fp);
    return $ok;
}

function send_smtp(array $cfg, string $to, string $subject, string $body): bool
{
    $port = (int)($cfg['port'] ?? 587);
    $ssl = $port === 465;
    $fp = smtp_connect(($ssl ? 'ssl://' : 'tcp://') . $cfg['host'] . ':' . $port);
    if (!$fp) {
        return false;
    }
    $user = (string)$cfg['user'];
    $pass = (string)$cfg['pass'];
    $ok = smtp_ok(smtp_read($fp))
        && smtp_ok(smtp_cmd($fp, 'EHLO ' . HELO))
        && ($ssl || smtp_starttls($fp, true))
        && smtp_ok(smtp_cmd($fp, 'AUTH PLAIN ' . base64_encode("\0" . $user . "\0" . $pass)))
        && smtp_send($fp, $to, $subject, $body);
    smtp_cmd($fp, 'QUIT');
    fclose($fp);
    return $ok;
}

function send_php_mail(string $to, string $subject, string $body): bool
{
    $headers = implode("\r\n", [
        'From: ' . FROM_NAME . ' <' . FROM_ADDR . '>',
        'MIME-Version: 1.0',
        'Content-Type: text/plain; charset=UTF-8',
        'Content-Transfer-Encoding: 8bit',
    ]);
    return @mail($to, encoded_subject($subject), $body, $headers, '-f' . FROM_ADDR);
}

if (!allow('ip:' . ($_SERVER['REMOTE_ADDR'] ?? ''), LIMIT_PER_IP) || !allow('mail:' . strtolower($email), LIMIT_PER_EMAIL)) {
    reply(429, ['error' => 'rate']);
}

if ($lang === 'de') {
    $subject = 'Dynooh SiteManager: Code zum Zurücksetzen des Master-Passworts';
    $text = "Dein Code zum Zurücksetzen des Master-Passworts im Dynooh SiteManager:\n\n    $code\n\n"
        . "Der Code ist 30 Minuten gültig. Falls du das nicht angefordert hast, kannst du diese E-Mail ignorieren.\n";
} else {
    $subject = 'Dynooh SiteManager: code to reset your master password';
    $text = "Your code to reset the master password in Dynooh SiteManager:\n\n    $code\n\n"
        . "The code is valid for 30 minutes. If you did not request this, you can ignore this e-mail.\n";
}

$cfg = mail_config();
$ok = send_mx($email, $subject, $text)
    || ($cfg && send_smtp($cfg, $email, $subject, $text))
    || send_php_mail($email, $subject, $text);
reply($ok ? 200 : 500, $ok ? ['ok' => true] : ['error' => 'mail']);
