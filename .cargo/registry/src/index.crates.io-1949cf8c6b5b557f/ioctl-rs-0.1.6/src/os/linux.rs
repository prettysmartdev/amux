use libc::c_int;

// socket

pub const FIOSETOWN: c_int = 0x00008901;
pub const SIOCSPGRP: c_int = 0x00008902;
pub const FIOGETOWN: c_int = 0x00008903;
pub const SIOCGPGRP: c_int = 0x00008904;
pub const SIOCGSTAMP: c_int = 0x00008906;

// termios

pub const TCGETS: c_int = 0x00005401;
pub const TCSETS: c_int = 0x00005402;
pub const TCSETSW: c_int = 0x00005403;
pub const TCSETSF: c_int = 0x00005404;
pub const TCGETA: c_int = 0x00005405;
pub const TCSETA: c_int = 0x00005406;
pub const TCSETAW: c_int = 0x00005407;
pub const TCSETAF: c_int = 0x00005408;
pub const TCSBRK: c_int = 0x00005409;
pub const TCXONC: c_int = 0x0000540a;
pub const TCFLSH: c_int = 0x0000540b;
pub const TIOCEXCL: c_int = 0x0000540c;
pub const TIOCNXCL: c_int = 0x0000540d;
pub const TIOCSCTTY: c_int = 0x0000540e;
pub const TIOCGPGRP: c_int = 0x0000540f;
pub const TIOCSPGRP: c_int = 0x00005410;
pub const TIOCOUTQ: c_int = 0x00005411;
pub const TIOCSTI: c_int = 0x00005412;
pub const TIOCGWINSZ: c_int = 0x00005413;
pub const TIOCSWINSZ: c_int = 0x00005414;
pub const TIOCMGET: c_int = 0x00005415;
pub const TIOCMBIS: c_int = 0x00005416;
pub const TIOCMBIC: c_int = 0x00005417;
pub const TIOCMSET: c_int = 0x00005418;
pub const TIOCGSOFTCAR: c_int = 0x00005419;
pub const TIOCSSOFTCAR: c_int = 0x0000541a;
pub const FIONREAD: c_int = 0x0000541b;
pub const TIOCINQ: c_int = 0x0000541b;
pub const TIOCLINUX: c_int = 0x0000541c;
pub const TIOCCONS: c_int = 0x0000541d;
pub const TIOCGSERIAL: c_int = 0x0000541e;
pub const TIOCSSERIAL: c_int = 0x0000541f;
pub const TIOCPKT: c_int = 0x00005420;
pub const FIONBIO: c_int = 0x00005421;
pub const TIOCNOTTY: c_int = 0x00005422;
pub const TIOCSETD: c_int = 0x00005423;
pub const TIOCGETD: c_int = 0x00005424;
pub const TCSBRKP: c_int = 0x00005425;
pub const FIONCLEX: c_int = 0x00005450;
pub const FIOCLEX: c_int = 0x00005451;
pub const FIOASYNC: c_int = 0x00005452;
pub const TIOCSERCONFIG: c_int = 0x00005453;
pub const TIOCSERGWILD: c_int = 0x00005454;
pub const TIOCSERSWILD: c_int = 0x00005455;
pub const TIOCGLCKTRMIOS: c_int = 0x00005456;
pub const TIOCSLCKTRMIOS: c_int = 0x00005457;
pub const TIOCSERGSTRUCT: c_int = 0x00005458;
pub const TIOCSERGETLSR: c_int = 0x00005459;
pub const TIOCSERGETMULTI: c_int = 0x0000545a;
pub const TIOCSERSETMULTI: c_int = 0x0000545b;

// sockios

pub const SIOCADDRT: c_int = 0x0000890b;
pub const SIOCDELRT: c_int = 0x0000890c;
pub const SIOCGIFNAME: c_int = 0x00008910;
pub const SIOCSIFLINK: c_int = 0x00008911;
pub const SIOCGIFCONF: c_int = 0x00008912;
pub const SIOCGIFFLAGS: c_int = 0x00008913;
pub const SIOCSIFFLAGS: c_int = 0x00008914;
pub const SIOCGIFADDR: c_int = 0x00008915;
pub const SIOCSIFADDR: c_int = 0x00008916;
pub const SIOCGIFDSTADDR: c_int = 0x00008917;
pub const SIOCSIFDSTADDR: c_int = 0x00008918;
pub const SIOCGIFBRDADDR: c_int = 0x00008919;
pub const SIOCSIFBRDADDR: c_int = 0x0000891a;
pub const SIOCGIFNETMASK: c_int = 0x0000891b;
pub const SIOCSIFNETMASK: c_int = 0x0000891c;
pub const SIOCGIFMETRIC: c_int = 0x0000891d;
pub const SIOCSIFMETRIC: c_int = 0x0000891e;
pub const SIOCGIFMEM: c_int = 0x0000891f;
pub const SIOCSIFMEM: c_int = 0x00008920;
pub const SIOCGIFMTU: c_int = 0x00008921;
pub const SIOCSIFMTU: c_int = 0x00008922;
pub const SIOCSIFHWADDR: c_int = 0x00008924;
pub const SIOCGIFENCAP: c_int = 0x00008925;
pub const SIOCSIFENCAP: c_int = 0x00008926;
pub const SIOCGIFHWADDR: c_int = 0x00008927;
pub const SIOCGIFSLAVE: c_int = 0x00008929;
pub const SIOCSIFSLAVE: c_int = 0x00008930;
pub const SIOCADDMULTI: c_int = 0x00008931;
pub const SIOCDELMULTI: c_int = 0x00008932;
pub const SIOCDARP: c_int = 0x00008953;
pub const SIOCGARP: c_int = 0x00008954;
pub const SIOCSARP: c_int = 0x00008955;
pub const SIOCDRARP: c_int = 0x00008960;
pub const SIOCGRARP: c_int = 0x00008961;
pub const SIOCSRARP: c_int = 0x00008962;
pub const SIOCGIFMAP: c_int = 0x00008970;
pub const SIOCSIFMAP: c_int = 0x00008971;

// modem control lines

pub const TIOCM_LE: c_int = 0x00000001;
pub const TIOCM_DTR: c_int = 0x00000002;
pub const TIOCM_RTS: c_int = 0x00000004;
pub const TIOCM_ST: c_int = 0x00000008;
pub const TIOCM_SR: c_int = 0x00000010;
pub const TIOCM_CTS: c_int = 0x00000020;
pub const TIOCM_CAR: c_int = 0x00000040;
pub const TIOCM_CD: c_int = 0x00000040;
pub const TIOCM_RNG: c_int = 0x00000080;
pub const TIOCM_RI: c_int = 0x00000080;
pub const TIOCM_DSR: c_int = 0x00000100;

extern "C" {
    pub fn ioctl(fd: c_int, request: c_int, ...) -> c_int;
}
