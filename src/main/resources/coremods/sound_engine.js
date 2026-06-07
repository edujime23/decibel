function initializeCoreMod() {
    print("[Decibel CoreMod] Initializing JavaScript CoreMod...");

    return {
        'decibel_sound_bypass': {
            'target': {
                'type': 'METHOD',
                'class': 'net.minecraft.client.sounds.SoundEngine',
                'methodName': 'play',
                'methodDesc': '(Lnet/minecraft/client/resources/sounds/SoundInstance;)V'
            },
            'transformer': function(methodNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var VarInsnNode = Java.type('org.objectweb.asm.tree.VarInsnNode');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var JumpInsnNode = Java.type('org.objectweb.asm.tree.JumpInsnNode');
                var LabelNode = Java.type('org.objectweb.asm.tree.LabelNode');
                var InsnNode = Java.type('org.objectweb.asm.tree.InsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Splicing net.minecraft.client.sounds.SoundEngine.play()...");

                var instructions = new InsnList();
                instructions.add(new VarInsnNode(Opcodes.ALOAD, 1));
                instructions.add(new MethodInsnNode(
                    Opcodes.INVOKESTATIC,
                    "com/edujime23/decibel/asm/SoundInterceptor",
                    "onPlaySound",
                    "(Lnet/minecraft/client/resources/sounds/SoundInstance;)Z",
                    false
                ));

                var label = new LabelNode();
                instructions.add(new JumpInsnNode(Opcodes.IFEQ, label));
                instructions.add(new InsnNode(Opcodes.RETURN));
                instructions.add(label);

                methodNode.instructions.insert(instructions);
                return methodNode;
            }
        },
        'decibel_sound_stop': {
            'target': {
                'type': 'METHOD',
                'class': 'net.minecraft.client.sounds.SoundEngine',
                'methodName': 'stop',
                'methodDesc': '(Lnet/minecraft/client/resources/sounds/SoundInstance;)V'
            },
            'transformer': function(methodNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var VarInsnNode = Java.type('org.objectweb.asm.tree.VarInsnNode');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var JumpInsnNode = Java.type('org.objectweb.asm.tree.JumpInsnNode');
                var LabelNode = Java.type('org.objectweb.asm.tree.LabelNode');
                var InsnNode = Java.type('org.objectweb.asm.tree.InsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Splicing net.minecraft.client.sounds.SoundEngine.stop()...");

                var instructions = new InsnList();
                instructions.add(new VarInsnNode(Opcodes.ALOAD, 1));
                instructions.add(new MethodInsnNode(
                    Opcodes.INVOKESTATIC,
                    "com/edujime23/decibel/asm/SoundInterceptor",
                    "onStopSound",
                    "(Lnet/minecraft/client/resources/sounds/SoundInstance;)Z",
                    false
                ));

                var label = new LabelNode();
                instructions.add(new JumpInsnNode(Opcodes.IFEQ, label));
                instructions.add(new InsnNode(Opcodes.RETURN));
                instructions.add(label);

                methodNode.instructions.insert(instructions);
                return methodNode;
            }
        },
        'decibel_listener_update': {
            'target': {
                'type': 'METHOD',
                'class': 'net.minecraft.client.sounds.SoundEngine',
                'methodName': 'updateSource',
                'methodDesc': '(Lnet/minecraft/client/Camera;)V'
            },
            'transformer': function(methodNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var VarInsnNode = Java.type('org.objectweb.asm.tree.VarInsnNode');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Splicing net.minecraft.client.sounds.SoundEngine.updateSource()...");

                var instructions = new InsnList();
                instructions.add(new VarInsnNode(Opcodes.ALOAD, 1));
                instructions.add(new MethodInsnNode(
                    Opcodes.INVOKESTATIC,
                    "com/edujime23/decibel/asm/SoundInterceptor",
                    "onUpdateListener",
                    "(Lnet/minecraft/client/Camera;)V",
                    false
                ));

                methodNode.instructions.insert(instructions);
                return methodNode;
            }
        },
        'decibel_sound_stop_all': {
            'target': {
                'type': 'METHOD',
                'class': 'net.minecraft.client.sounds.SoundEngine',
                'methodName': 'stopAll',
                'methodDesc': '()V'
            },
            'transformer': function(methodNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Splicing net.minecraft.client.sounds.SoundEngine.stopAll()...");

                var instructions = new InsnList();
                instructions.add(new MethodInsnNode(
                    Opcodes.INVOKESTATIC,
                    "com/edujime23/decibel/asm/SoundInterceptor",
                    "onStopAll",
                    "()V",
                    false
                ));

                methodNode.instructions.insert(instructions);
                return methodNode;
            }
        },
        'decibel_sound_destroy': {
            'target': {
                'type': 'METHOD',
                'class': 'net.minecraft.client.sounds.SoundEngine',
                'methodName': 'destroy',
                'methodDesc': '()V'
            },
            'transformer': function(methodNode) {
                var Opcodes = Java.type('org.objectweb.asm.Opcodes');
                var MethodInsnNode = Java.type('org.objectweb.asm.tree.MethodInsnNode');
                var InsnList = Java.type('org.objectweb.asm.tree.InsnList');

                print("[Decibel CoreMod] Splicing net.minecraft.client.sounds.SoundEngine.destroy()...");

                var instructions = new InsnList();
                instructions.add(new MethodInsnNode(
                    Opcodes.INVOKESTATIC,
                    "com/edujime23/decibel/asm/SoundInterceptor",
                    "onStopAll",
                    "()V",
                    false
                ));

                methodNode.instructions.insert(instructions);
                return methodNode;
            }
        }
    }
}