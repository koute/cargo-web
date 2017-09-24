#!/usr/bin/ruby

require "fileutils"
require "shellwords"
require "pathname"

def development_mode?
    (ENV[ "DEV_MODE" ] || "").strip == "1"
end

def relative path
    return path unless path.start_with? "/"
    Pathname.new( path ).relative_path_from( Pathname.new( Dir.pwd ) )
end

def fail message
    STDERR.puts message
    exit 1
end

def run command
    command = command.strip
    puts "> #{command.gsub( /^ +/, "" ).gsub( "\n", " \\\n>    " )}"
    system command.gsub( "\n", "\\\n" )
    fail "Command failed with status #{$?.exitstatus}" if $?.exitstatus != 0
end

def mkdir_p path
    return if File.exist? path

    puts "> mkdir -p #{relative path}"
    FileUtils.mkdir_p path
end

def mkdir_clean path
    rm_rf path unless development_mode?
    mkdir_p path
    File.expand_path path
end

def rm_rf path
    return unless File.exist? path

    puts "> rm -Rf #{relative path}"
    FileUtils.rm_rf path
end

def rm_f path
    return unless File.exist? path

    puts "> rm -f #{relative path}"
    FileUtils.rm_f path
end

def mv src, dst
    puts "> mv #{relative src} #{relative dst}"
    FileUtils.mv src, dst
end

def ln_s target, path
    puts "> ln -s #{target} #{relative path}"
    FileUtils.ln_s target, path
end

def chdir path, &block
    puts "> chdir #{relative path}"
    Dir.chdir( path ) do
        block.call
    end
    puts "> chdir .."
end

def export name, value
    puts "> export #{name}=#{value}"
    ENV[ name ] = value
end

ARCH = ENV[ "ARCH" ]
fail "No $ARCH specified!" if ARCH == nil

TMP_ROOT = mkdir_clean "tmp-root"

os = `uname -o`.strip
if os == "GNU/Linux"
    if ARCH == "i686"
        TARGET = "i686-unknown-linux-gnu"
        EXTRA_CFLAGS = "-m32 -I/tmp/tools/usr/local/include"

        unless File.exist? "/usr/include/asm"
            mkdir_p "#{TMP_ROOT}/usr/local/include"
            ln_s "/usr/include/asm-generic", "#{TMP_ROOT}/usr/local/include/asm"
        end
    elsif ARCH == "x86_64"
        TARGET = "x86_64-unknown-linux-gnu"
        EXTRA_CFLAGS = "-m64"
    else
        fail "Unknown $ARCH: '#{ARCH}'"
    end

    CPU_COUNT = `grep -c ^processor /proc/cpuinfo`.to_i
else
    fail "Unknown OS: '#{os}'"
end

require_relative './config.rb'

fail "NAME not defined in config.rb" unless Kernel.const_defined?( "NAME" )
fail "VERSION not defined in config.rb" unless Kernel.const_defined?( "VERSION" )
fail "RELEASE not defined in config.rb" unless Kernel.const_defined?( "RELEASE" )

OUTPUT = "#{NAME}-#{VERSION}-#{RELEASE}-#{TARGET}.tgz"

mkdir_p "#{TMP_ROOT}/usr/local/bin"
export "PATH", "#{TMP_ROOT}/usr/local/bin:#{ENV[ "PATH" ]}"

if Kernel.const_defined?( "INSTALL_CMAKE" ) && INSTALL_CMAKE
    run "curl --stderr - -Lo cmake-3.7.2-Linux-x86_64.sh https://cmake.org/files/v3.7/cmake-3.7.2-Linux-x86_64.sh"
    run "echo '2e250d9a23764a9c54262c1ddc275b02666b81e2cbe05459d15906825655874b  cmake-3.7.2-Linux-x86_64.sh' | sha256sum -c"

    run "chmod +x ./cmake-3.7.2-Linux-x86_64.sh"
    run "./cmake-3.7.2-Linux-x86_64.sh --skip-license --prefix=#{TMP_ROOT}/usr/local"
    run "rm cmake-3.7.2-Linux-x86_64.sh"
end

BUILD_DIR = mkdir_clean "#{NAME}-#{TARGET}-build"
DESTDIR = mkdir_clean "#{NAME}-#{TARGET}-destdir"

chdir( BUILD_DIR ) do
    FILES.each do |url, checksum, target|
        run "curl --stderr - -Lo #{target.shellescape} #{url.shellescape}"
        run "echo '#{checksum}  #{target}' | sha256sum -c"
    end

    require_relative "./build.rb"
end

chdir( DESTDIR ) do
    rm_f "../#{OUTPUT}"
    run "tar -zcf ../#{OUTPUT} *"
end

run "sha256sum #{OUTPUT}"
run "sha256sum #{OUTPUT} > #{OUTPUT}.sha256"

run "tar -cf output.tar #{OUTPUT} #{OUTPUT}.sha256"

unless development_mode?
    rm_rf BUILD_DIR
    rm_rf DESTDIR
    rm_rf TMP_ROOT
end
